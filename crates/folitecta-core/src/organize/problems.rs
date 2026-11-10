//! Show Problem Details: what is wrong in the file as it stands, page by
//! page — references to objects the file does not hold, page contents that
//! cannot be read, fonts the text uses that the page does not name, comments
//! without an area or a kind, a page count that disagrees with the pages
//! found. Nothing is changed; each problem says where it is.

use std::collections::BTreeSet;

use lopdf::content::Content;
use lopdf::{Object, ObjectId};

use crate::document::Document;

/// A problem of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The page (zero-based), when it is on one.
    pub page: Option<usize>,
    /// What kind of problem: `missing-object`, `unreadable-content`,
    /// `missing-font`, `comment-without-area`, `comment-without-kind`,
    /// `page-count`.
    pub kind: &'static str,
    /// What it is about: an object number, a font name, a count.
    pub detail: String,
}

/// The references inside `o` (followed into arrays, dictionaries and the
/// dictionaries of streams).
fn references(o: &Object, out: &mut Vec<ObjectId>) {
    match o {
        Object::Reference(r) => out.push(*r),
        Object::Array(a) => a.iter().for_each(|x| references(x, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, v)| references(v, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, v)| references(v, out)),
        _ => {}
    }
}

impl Document {
    /// What is wrong in the file (see the module); empty when nothing is.
    pub fn problems(&self) -> Vec<Problem> {
        let doc = &self.inner;
        let mut out = Vec::new();
        // References to objects the file does not hold, each once.
        let mut missing: BTreeSet<ObjectId> = BTreeSet::new();
        let mut refs = Vec::new();
        for o in doc.objects.values() {
            references(o, &mut refs);
        }
        references(&Object::Dictionary(doc.trailer.clone()), &mut refs);
        for r in refs {
            if !doc.objects.contains_key(&r) {
                missing.insert(r);
            }
        }
        for r in missing {
            out.push(Problem {
                page: None,
                kind: "missing-object",
                detail: format!("{} {}", r.0, r.1),
            });
        }
        let ids = self.page_ids();
        // The count the page tree states, against the pages found.
        let stated = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Pages").ok())
            .and_then(|p| p.as_reference().ok())
            .and_then(|p| doc.get_dictionary(p).ok())
            .and_then(|p| p.get(b"Count").ok())
            .and_then(|c| c.as_i64().ok());
        if let Some(n) = stated.filter(|n| *n != ids.len() as i64) {
            out.push(Problem {
                page: None,
                kind: "page-count",
                detail: format!("{n} / {}", ids.len()),
            });
        }
        for (page, id) in ids.iter().enumerate() {
            let data = doc.get_page_content(*id);
            match Content::decode(&data) {
                Err(e) => out.push(Problem {
                    page: Some(page),
                    kind: "unreadable-content",
                    detail: e.to_string(),
                }),
                Ok(content) => {
                    let named = doc.get_page_fonts(*id).unwrap_or_default();
                    let mut used: BTreeSet<Vec<u8>> = BTreeSet::new();
                    for op in &content.operations {
                        if op.operator == "Tf" {
                            if let Some(Ok(name)) = op.operands.first().map(|o| o.as_name()) {
                                used.insert(name.to_vec());
                            }
                        }
                    }
                    for name in used {
                        if !named.contains_key(&name) {
                            out.push(Problem {
                                page: Some(page),
                                kind: "missing-font",
                                detail: String::from_utf8_lossy(&name).to_string(),
                            });
                        }
                    }
                }
            }
            let annots = doc
                .get_dictionary(*id)
                .ok()
                .and_then(|d| d.get(b"Annots").ok())
                .map(|a| crate::util::deref(doc, a))
                .and_then(|a| a.as_array().ok())
                .cloned()
                .unwrap_or_default();
            for (k, a) in annots.iter().enumerate() {
                let Ok(d) = crate::util::deref(doc, a).as_dict() else {
                    continue;
                };
                let kind = d
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    .map(|s| String::from_utf8_lossy(s).to_string());
                match kind {
                    None => out.push(Problem {
                        page: Some(page),
                        kind: "comment-without-kind",
                        detail: (k + 1).to_string(),
                    }),
                    Some(kind) if !d.has(b"Rect") => out.push(Problem {
                        page: Some(page),
                        kind: "comment-without-area",
                        detail: format!("{} ({kind})", k + 1),
                    }),
                    Some(_) => {}
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdocs::Builder;
    use lopdf::{Dictionary, dictionary};

    #[test]
    fn sound_files_have_no_problems() {
        use crate::testdocs::*;
        let sample = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/sample.pdf")).unwrap();
        for (name, bytes) in [
            ("text", text_doc(2)),
            ("report", report_doc(2)),
            ("form", form_doc(2, 3)),
            ("relations", relations_doc()),
            ("fonts", fonts_doc()),
            ("shared picture", shared_picture_doc()),
            ("sample", sample),
        ] {
            let doc = Document::from_bytes(bytes, None, None).unwrap();
            assert_eq!(doc.problems(), vec![], "{name}");
        }
    }

    #[test]
    fn missing_objects_fonts_and_comment_areas_are_listed() {
        let mut b = Builder::new();
        // Page 1 uses /F9, which it does not name; page 2 has a comment
        // without an area and points to an object that is not there.
        b.page(
            (200.0, 200.0),
            b"BT /F9 12 Tf 10 10 Td (x) Tj ET".to_vec(),
            Dictionary::new(),
            vec![],
        );
        let annot = Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Square",
            "Popup" => Object::Reference((999, 0)),
        });
        b.page((200.0, 200.0), Vec::new(), Dictionary::new(), vec![annot]);
        let doc = Document::from_bytes(b.finish(), None, None).unwrap();
        let found = doc.problems();
        let kinds: Vec<(Option<usize>, &str)> = found.iter().map(|p| (p.page, p.kind)).collect();
        assert!(kinds.contains(&(None, "missing-object")), "{found:?}");
        assert!(kinds.contains(&(Some(0), "missing-font")), "{found:?}");
        assert!(kinds.contains(&(Some(1), "comment-without-area")), "{found:?}");
        assert_eq!(
            found.iter().find(|p| p.kind == "missing-font").unwrap().detail,
            "F9"
        );
        assert_eq!(
            found.iter().find(|p| p.kind == "missing-object").unwrap().detail,
            "999 0"
        );
    }
}

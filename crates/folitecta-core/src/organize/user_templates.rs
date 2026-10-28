//! The user's own templates. A document saved as a template keeps, in its
//! document information, the template's name, the file it was made from,
//! when, with which version of Folitecta, and the fonts it uses without
//! carrying them; a new document made from a template is a copy with no
//! file yet, so saving it never changes the template.

use std::collections::BTreeSet;

use lopdf::Object;

use crate::document::Document;
use crate::error::{Error, Result};
use crate::util::{deref, dict_text};

/// What a template file says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserTemplate {
    pub name: String,
    /// The file the template was made from (its name), if it had one.
    pub origin: String,
    /// When it was made (a PDF date).
    pub made: String,
    /// The version of Folitecta that made it.
    pub version: String,
    /// Fonts its pages use without carrying them (the reader must have them).
    pub fonts: Vec<String>,
    pub pages: usize,
}

const NAME: &[u8] = b"FolitectaTemplate";
const ORIGIN: &[u8] = b"FolitectaTemplateOrigin";
const MADE: &[u8] = b"FolitectaTemplateMade";
const VERSION: &[u8] = b"FolitectaTemplateVersion";
const FONTS: &[u8] = b"FolitectaTemplateFonts";

/// The fonts every reader has (the standard 14), so no template needs them.
const STANDARD: [&str; 14] = [
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Symbol",
    "ZapfDingbats",
];

impl Document {
    /// The fonts of the document whose programs it does not carry, the
    /// standard 14 left out.
    pub fn fonts_not_carried(&self) -> Vec<String> {
        let doc = &self.inner;
        let carried = |d: &lopdf::Dictionary| {
            d.get(b"FontDescriptor")
                .ok()
                .and_then(|f| deref(doc, f).as_dict().ok())
                .is_some_and(|f| f.has(b"FontFile") || f.has(b"FontFile2") || f.has(b"FontFile3"))
        };
        // Font dictionaries, objects of their own or written inside others.
        fn collect<'a>(o: &'a Object, out: &mut Vec<&'a lopdf::Dictionary>, depth: usize) {
            if depth > 16 {
                return;
            }
            let d = match o {
                Object::Dictionary(d) => d,
                Object::Stream(s) => &s.dict,
                Object::Array(a) => {
                    a.iter().for_each(|x| collect(x, out, depth + 1));
                    return;
                }
                _ => return,
            };
            if d.get(b"Type").and_then(|t| t.as_name()).ok() == Some(b"Font".as_slice()) {
                out.push(d);
            }
            d.iter().for_each(|(_, v)| collect(v, out, depth + 1));
        }
        let mut dicts = Vec::new();
        doc.objects.values().for_each(|o| collect(o, &mut dicts, 0));
        let mut out = BTreeSet::new();
        for d in dicts {
            let subtype = d.get(b"Subtype").and_then(|t| t.as_name()).unwrap_or(b"");
            if subtype == b"Type3" {
                continue;
            }
            let Some(base) = d.get(b"BaseFont").ok().and_then(|b| b.as_name().ok()) else {
                continue;
            };
            let base = String::from_utf8_lossy(base).to_string();
            // A composite font carries its program in its descendant.
            let ok = if subtype == b"Type0" {
                d.get(b"DescendantFonts")
                    .ok()
                    .and_then(|a| deref(doc, a).as_array().ok())
                    .and_then(|a| a.first())
                    .and_then(|f| deref(doc, f).as_dict().ok())
                    .is_some_and(carried)
            } else {
                carried(d)
            };
            if !ok && !STANDARD.contains(&base.as_str()) {
                out.insert(base);
            }
        }
        out.into_iter().collect()
    }

    /// The file of this document as a template named `name` (see the
    /// module). A protected document cannot be one: the template would keep
    /// its pages without their protection.
    pub fn template_bytes(&self, name: &str) -> Result<Vec<u8>> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("the template has no name".into()));
        }
        if self.is_protected() {
            return Err(Error::Invalid(
                "a protected document cannot be a template: the template would keep its pages without the protection".into(),
            ));
        }
        let mut copy = self.derived()?;
        copy.changing();
        let fonts = copy.fonts_not_carried();
        let origin = self
            .path()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let info = copy.info_dict_mut()?;
        info.set(NAME.to_vec(), lopdf::text_string(name));
        info.set(ORIGIN.to_vec(), lopdf::text_string(&origin));
        info.set(MADE.to_vec(), Object::string_literal(crate::util::pdf_date_now()));
        info.set(VERSION.to_vec(), Object::string_literal(crate::VERSION));
        info.set(FONTS.to_vec(), lopdf::text_string(&fonts.join(", ")));
        copy.commit()?;
        Ok(copy.bytes().to_vec())
    }

    /// What this document says about itself as a template (`None` when it
    /// was not saved as one).
    pub fn template_info(&self) -> Option<UserTemplate> {
        let info = self.info_dict()?;
        let get = |k: &[u8]| dict_text(&self.inner, info, k).unwrap_or_default();
        let name = get(NAME);
        if name.is_empty() {
            return None;
        }
        Some(UserTemplate {
            name,
            origin: get(ORIGIN),
            made: get(MADE),
            version: get(VERSION),
            fonts: get(FONTS)
                .split(", ")
                .filter(|f| !f.is_empty())
                .map(str::to_string)
                .collect(),
            pages: self.page_count(),
        })
    }

    /// A new document from the template file `bytes`: the same pages, fields
    /// and comments, without the template's own entries and with no file,
    /// so it is saved somewhere new.
    pub fn from_template_file(bytes: Vec<u8>) -> Result<Document> {
        let mut doc = Document::from_bytes(bytes, None, None)?;
        if doc.template_info().is_none() {
            return Err(Error::Invalid("the file is not a template".into()));
        }
        doc.begin_edit();
        if let Ok(info) = doc.info_dict_mut() {
            for k in [NAME, ORIGIN, MADE, VERSION, FONTS] {
                info.remove(k);
            }
        }
        doc.commit()?;
        doc.set_dirty_new();
        doc.clear_history();
        Ok(doc)
    }
}

/// The template file `bytes` under the name `name` (what else it says
/// stays as it was).
pub fn renamed_template(bytes: Vec<u8>, name: &str) -> Result<Vec<u8>> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("the template has no name".into()));
    }
    let mut doc = Document::from_bytes(bytes, None, None)?;
    if doc.template_info().is_none() {
        return Err(Error::Invalid("the file is not a template".into()));
    }
    doc.begin_edit();
    doc.info_dict_mut()?.set(NAME.to_vec(), lopdf::text_string(name));
    doc.commit()?;
    Ok(doc.bytes().to_vec())
}

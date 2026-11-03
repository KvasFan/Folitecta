//! The fonts a document draws with: those of its pages, of the forms and
//! patterns they use and of the appearances of comments and fields, each
//! with its kind, whether the file holds it and the pages it appears on.
//! A font, graphics state or form a resource dictionary lists counts only
//! when the content that owns the resources names it (`Tf`, `gs`, `Do`);
//! when that content cannot be read, everything listed counts.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::Document;
use crate::page_text::probe::page_resources;
use crate::util::deref;

/// The kind of a font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontKind {
    Type1,
    TrueType,
    /// A Type 0 font over a descendant font (CID-keyed).
    Composite,
    /// Glyphs drawn by content streams of the file.
    Type3,
    Other,
}

impl FontKind {
    /// The id of the kind, for the interface's words.
    pub fn id(self) -> &'static str {
        match self {
            FontKind::Type1 => "type1",
            FontKind::TrueType => "truetype",
            FontKind::Composite => "composite",
            FontKind::Type3 => "type3",
            FontKind::Other => "other",
        }
    }
}

/// A font of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedFont {
    /// The name, without the tag of a subset.
    pub name: String,
    pub kind: FontKind,
    /// The file holds its glyphs.
    pub embedded: bool,
    /// Only the glyphs in use are in the file.
    pub subset: bool,
    /// One of the 14 fonts readers bring (when not embedded).
    pub standard: bool,
    /// The pages it appears on (zero-based, ascending).
    pub pages: Vec<usize>,
}

const STANDARD_FONTS: [&str; 14] = [
    "Courier",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Courier-Oblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Helvetica-Oblique",
    "Times-Roman",
    "Times-Bold",
    "Times-BoldItalic",
    "Times-Italic",
    "Symbol",
    "ZapfDingbats",
];

/// The name without the tag of a subset (six capitals and a plus).
fn subset_name(name: &str) -> Option<&str> {
    let (tag, rest) = name.split_once('+')?;
    (tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase())).then_some(rest)
}

type Key = (String, FontKind, bool, bool);

/// What the walk found: fonts by what tells them apart, with their pages.
type Found = BTreeMap<Key, BTreeSet<usize>>;

fn key_of(doc: &lopdf::Document, font: &Dictionary, resource: &[u8]) -> Key {
    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|s| s.as_name().ok())
        .unwrap_or(b"");
    let kind = match subtype {
        b"Type1" | b"MMType1" => FontKind::Type1,
        b"TrueType" => FontKind::TrueType,
        b"Type0" => FontKind::Composite,
        b"Type3" => FontKind::Type3,
        _ => FontKind::Other,
    };
    let base = font
        .get(if kind == FontKind::Type3 {
            b"Name".as_slice()
        } else {
            b"BaseFont".as_slice()
        })
        .ok()
        .and_then(|n| deref(doc, n).as_name().ok())
        .unwrap_or(resource);
    let name = String::from_utf8_lossy(base).to_string();
    let descendant = font
        .get(b"DescendantFonts")
        .ok()
        .and_then(|d| deref(doc, d).as_array().ok())
        .and_then(|a| a.first())
        .and_then(|f| deref(doc, f).as_dict().ok());
    let embedded = kind == FontKind::Type3
        || descendant
            .unwrap_or(font)
            .get(b"FontDescriptor")
            .ok()
            .and_then(|d| deref(doc, d).as_dict().ok())
            .is_some_and(|d| d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3"));
    match subset_name(&name) {
        Some(short) => (short.to_string(), kind, embedded, true),
        None => (name, kind, embedded, false),
    }
}

fn dict_of<'a>(doc: &'a lopdf::Document, o: &'a Object) -> Option<&'a Dictionary> {
    match deref(doc, o) {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

/// The names a content stream uses: fonts (`Tf`), graphics states (`gs`)
/// and forms and pictures (`Do`). `None` when it cannot be read.
struct Used {
    fonts: HashSet<Vec<u8>>,
    states: HashSet<Vec<u8>>,
    forms: HashSet<Vec<u8>>,
}

fn used_names(content: &[u8]) -> Option<Used> {
    let ops = lopdf::content::Content::decode(content).ok()?.operations;
    let mut used = Used {
        fonts: HashSet::new(),
        states: HashSet::new(),
        forms: HashSet::new(),
    };
    for op in ops {
        let set = match op.operator.as_str() {
            "Tf" => &mut used.fonts,
            "gs" => &mut used.states,
            "Do" => &mut used.forms,
            _ => continue,
        };
        if let Some(Object::Name(n)) = op.operands.first() {
            set.insert(n.clone());
        }
    }
    Some(used)
}

/// Whether `name` of a resource dictionary is used (everything is when the
/// content could not be read).
fn uses(used: Option<&Used>, pick: impl Fn(&Used) -> &HashSet<Vec<u8>>, name: &[u8]) -> bool {
    used.is_none_or(|u| pick(u).contains(name))
}

/// The fonts of `res` that `used` names (all when `None`), and of what it
/// draws with.
fn walk(
    doc: &lopdf::Document,
    res: &Dictionary,
    used: Option<&Used>,
    page: usize,
    seen: &mut HashSet<ObjectId>,
    found: &mut Found,
) {
    if let Some(fonts) = res.get(b"Font").ok().and_then(|f| dict_of(doc, f)) {
        for (name, font) in fonts.iter() {
            if !uses(used, |u| &u.fonts, name) {
                continue;
            }
            if let Some(font) = dict_of(doc, font) {
                found.entry(key_of(doc, font, name)).or_default().insert(page);
            }
        }
    }
    if let Some(states) = res.get(b"ExtGState").ok().and_then(|s| dict_of(doc, s)) {
        for (name, state) in states.iter() {
            if !uses(used, |u| &u.states, name) {
                continue;
            }
            let font = dict_of(doc, state)
                .and_then(|s| s.get(b"Font").ok())
                .and_then(|f| deref(doc, f).as_array().ok())
                .and_then(|a| a.first())
                .and_then(|f| dict_of(doc, f));
            if let Some(font) = font {
                found.entry(key_of(doc, font, name)).or_default().insert(page);
            }
        }
    }
    for kind in [b"XObject".as_slice(), b"Pattern".as_slice()] {
        let Some(list) = res.get(kind).ok().and_then(|l| dict_of(doc, l)) else {
            continue;
        };
        for (name, item) in list.iter() {
            // Patterns are named by colour operators: all of them count.
            if kind == b"XObject" && !uses(used, |u| &u.forms, name) {
                continue;
            }
            drawn(doc, item, page, seen, found);
        }
    }
}

/// The fonts of a form, pattern or appearance, once for each page.
fn drawn(doc: &lopdf::Document, item: &Object, page: usize, seen: &mut HashSet<ObjectId>, found: &mut Found) {
    if let Object::Reference(id) = item
        && !seen.insert(*id)
    {
        return;
    }
    if let Some(inner) = dict_of(doc, item)
        .and_then(|d| d.get(b"Resources").ok())
        .and_then(|r| dict_of(doc, r))
    {
        // The content of the form, pattern or appearance.
        let used = match deref(doc, item) {
            Object::Stream(s) => s
                .decompressed_content()
                .ok()
                .or_else(|| Some(s.content.clone()))
                .and_then(|c| used_names(&c)),
            _ => None,
        };
        walk(doc, inner, used.as_ref(), page, seen, found);
    }
}

impl Document {
    /// The fonts the document draws with, by name.
    pub fn used_fonts(&self) -> Vec<UsedFont> {
        let doc = &self.inner;
        let mut found = Found::new();
        for page in 0..self.page_count() {
            let Ok(id) = self.page_id(page) else { continue };
            let mut seen = HashSet::new();
            let used = used_names(&doc.get_page_content(id));
            walk(
                doc,
                &page_resources(doc, id),
                used.as_ref(),
                page,
                &mut seen,
                &mut found,
            );
            let annots = doc
                .get_dictionary(id)
                .ok()
                .and_then(|d| d.get(b"Annots").ok())
                .and_then(|a| deref(doc, a).as_array().ok());
            for annot in annots.into_iter().flatten() {
                let normal = dict_of(doc, annot)
                    .and_then(|a| a.get(b"AP").ok())
                    .and_then(|ap| dict_of(doc, ap))
                    .and_then(|ap| ap.get(b"N").ok());
                let Some(normal) = normal else { continue };
                match deref(doc, normal) {
                    // The states of a check box or radio button.
                    Object::Dictionary(states) => {
                        for (_, state) in states.iter() {
                            drawn(doc, state, page, &mut seen, &mut found);
                        }
                    }
                    _ => drawn(doc, normal, page, &mut seen, &mut found),
                }
            }
        }
        let mut fonts: Vec<UsedFont> = found
            .into_iter()
            .map(|((name, kind, embedded, subset), pages)| UsedFont {
                standard: !embedded && STANDARD_FONTS.contains(&name.as_str()),
                name,
                kind,
                embedded,
                subset,
                pages: pages.into_iter().collect(),
            })
            .collect();
        fonts.sort_by_cached_key(|f| (f.name.to_lowercase(), f.kind, f.subset));
        fonts
    }
}

#[cfg(test)]
mod tests {
    use super::subset_name;

    #[test]
    fn the_tag_of_a_subset() {
        assert_eq!(subset_name("ABCDEF+Liberation"), Some("Liberation"));
        assert_eq!(subset_name("Liberation"), None);
        assert_eq!(subset_name("ABCDE+Short"), None);
        assert_eq!(subset_name("abcdef+Lower"), None);
    }
}

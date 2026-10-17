//! Accessibility check: rules about the document (tags, language, title,
//! bookmarks), its content (tagged content and annotations, tab order, text
//! encoding), form fields, alternate text, tables, lists and headings, with
//! automatic fixes where the answer does not need a person.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId};

use crate::document::Document;
use crate::error::Result;
use crate::outline::OutlineItem;
use crate::structure::{StructElem, StructKid, heading_level};
use crate::util::{deref, dict_text};

/// Result of a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckStatus {
    Passed,
    Failed,
    /// A person has to look.
    Manual,
}

/// How a failed rule is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    None,
    /// Fixed without questions.
    Auto,
    /// Fixed with an answer from a person (language, alternate text).
    Ask,
}

/// One thing a rule found (an element, an annotation, a page, a font).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CheckItem {
    pub label: String,
    pub page: Option<usize>,
    pub elem: Option<ObjectId>,
    pub annot: Option<(usize, usize)>,
    /// Area on the page (display space).
    pub rect: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckResult {
    pub rule: &'static str,
    pub category: &'static str,
    pub status: CheckStatus,
    pub items: Vec<CheckItem>,
}

pub struct Rule {
    pub id: &'static str,
    pub category: &'static str,
    pub fix: Fix,
}

pub const CATEGORIES: &[&str] = &[
    "document", "content", "forms", "alt", "tables", "lists", "headings",
];

pub const RULES: &[Rule] = &[
    Rule {
        id: "permission",
        category: "document",
        fix: Fix::None,
    },
    Rule {
        id: "image_only",
        category: "document",
        fix: Fix::None,
    },
    Rule {
        id: "tagged",
        category: "document",
        fix: Fix::Auto,
    },
    Rule {
        id: "logical_order",
        category: "document",
        fix: Fix::None,
    },
    Rule {
        id: "language",
        category: "document",
        fix: Fix::Ask,
    },
    Rule {
        id: "title",
        category: "document",
        fix: Fix::Auto,
    },
    Rule {
        id: "bookmarks",
        category: "document",
        fix: Fix::Auto,
    },
    Rule {
        id: "contrast",
        category: "document",
        fix: Fix::None,
    },
    Rule {
        id: "tagged_content",
        category: "content",
        fix: Fix::None,
    },
    Rule {
        id: "tagged_annotations",
        category: "content",
        fix: Fix::Auto,
    },
    Rule {
        id: "tab_order",
        category: "content",
        fix: Fix::Auto,
    },
    Rule {
        id: "encoding",
        category: "content",
        fix: Fix::None,
    },
    Rule {
        id: "multimedia",
        category: "content",
        fix: Fix::None,
    },
    Rule {
        id: "flicker",
        category: "content",
        fix: Fix::None,
    },
    Rule {
        id: "scripts",
        category: "content",
        fix: Fix::None,
    },
    Rule {
        id: "timed",
        category: "content",
        fix: Fix::None,
    },
    Rule {
        id: "navigation_links",
        category: "content",
        fix: Fix::None,
    },
    Rule {
        id: "tagged_fields",
        category: "forms",
        fix: Fix::Auto,
    },
    Rule {
        id: "field_descriptions",
        category: "forms",
        fix: Fix::Auto,
    },
    Rule {
        id: "figures_alt",
        category: "alt",
        fix: Fix::Ask,
    },
    Rule {
        id: "nested_alt",
        category: "alt",
        fix: Fix::None,
    },
    Rule {
        id: "alt_content",
        category: "alt",
        fix: Fix::None,
    },
    Rule {
        id: "alt_hides_annot",
        category: "alt",
        fix: Fix::None,
    },
    Rule {
        id: "other_alt",
        category: "alt",
        fix: Fix::Ask,
    },
    Rule {
        id: "table_rows",
        category: "tables",
        fix: Fix::None,
    },
    Rule {
        id: "table_cells",
        category: "tables",
        fix: Fix::None,
    },
    Rule {
        id: "table_headers",
        category: "tables",
        fix: Fix::None,
    },
    Rule {
        id: "table_regular",
        category: "tables",
        fix: Fix::None,
    },
    Rule {
        id: "table_summary",
        category: "tables",
        fix: Fix::None,
    },
    Rule {
        id: "list_items",
        category: "lists",
        fix: Fix::None,
    },
    Rule {
        id: "list_lbl_body",
        category: "lists",
        fix: Fix::None,
    },
    Rule {
        id: "heading_nesting",
        category: "headings",
        fix: Fix::None,
    },
];

pub fn rule(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

/// Annotations that need a tag (not pop-ups, printer marks or hidden ones).
fn needs_tag(subtype: &str, hidden: bool) -> bool {
    !matches!(subtype, "Popup" | "PrinterMark" | "TrapNet") && (!hidden || subtype == "Widget")
}

/// Each element with the role of its parent.
fn with_parents(tree: &StructElem) -> Vec<(&StructElem, String)> {
    fn go<'a>(e: &'a StructElem, parent: &str, out: &mut Vec<(&'a StructElem, String)>) {
        if !e.is_root() {
            out.push((e, parent.to_string()));
        }
        let role = if e.is_root() { "" } else { e.role.as_str() };
        for c in e.children() {
            go(c, role, out);
        }
    }
    let mut out = Vec::new();
    go(tree, "", &mut out);
    out
}

/// A field name as a description: the last part, spaces for separators.
fn describe_name(name: &str) -> String {
    let last = name.rsplit('.').next().unwrap_or(name);
    let mut out = String::new();
    let mut prev_lower = false;
    for c in last.chars() {
        if c == '_' || c == '-' {
            out.push(' ');
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        prev_lower = c.is_lowercase();
        out.push(c);
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = out.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

impl Document {
    fn elem_item(&self, e: &StructElem) -> CheckItem {
        let page = e.first_page();
        CheckItem {
            label: if e.title.is_empty() {
                e.tag.clone()
            } else {
                format!("{} \u{2013} {}", e.tag, e.title)
            },
            page,
            elem: Some(e.id),
            annot: None,
            rect: page.and_then(|p| self.struct_rect(e, p)),
        }
    }

    /// The document title shows in the title bar instead of the file name.
    fn display_doc_title(&self) -> bool {
        self.inner
            .catalog()
            .ok()
            .and_then(|c| c.get(b"ViewerPreferences").ok())
            .and_then(|v| deref(&self.inner, v).as_dict().ok())
            .and_then(|v| v.get(b"DisplayDocTitle").ok())
            .and_then(|b| b.as_bool().ok())
            .unwrap_or(false)
    }

    /// Primary language of the document (`/Lang` of the catalog).
    pub fn language(&self) -> String {
        self.inner
            .catalog()
            .ok()
            .and_then(|c| dict_text(&self.inner, c, b"Lang"))
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    /// Fonts of the pages that do not say which characters they draw.
    fn fonts_without_unicode(&self) -> Vec<CheckItem> {
        let doc = &self.inner;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (p, pid) in self.page_ids().into_iter().enumerate() {
            let Some(res) = self.inherited_attr(pid, b"Resources") else {
                continue;
            };
            let Ok(res) = res.as_dict() else { continue };
            let mut font_dicts: Vec<&Dictionary> = Vec::new();
            if let Some(fonts) = res.get(b"Font").ok().and_then(|f| deref(doc, f).as_dict().ok()) {
                font_dicts.push(fonts);
            }
            let mut fonts: Vec<(Option<ObjectId>, &Dictionary)> = Vec::new();
            for fd in font_dicts {
                for (_, f) in fd.iter() {
                    let id = f.as_reference().ok();
                    if let Ok(d) = deref(doc, f).as_dict() {
                        fonts.push((id, d));
                    }
                }
            }
            for (id, f) in fonts {
                if let Some(id) = id {
                    if !seen.insert(id) {
                        continue;
                    }
                }
                if f.has(b"ToUnicode") {
                    continue;
                }
                let subtype = f
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    .unwrap_or(b"");
                let encoding = f.get(b"Encoding").ok().map(|e| deref(doc, e));
                let ok = match subtype {
                    b"Type0" => matches!(
                        encoding.and_then(|e| e.as_name().ok()),
                        Some(n) if n != b"Identity-H" && n != b"Identity-V"
                    ),
                    b"Type3" => matches!(encoding, Some(Object::Dictionary(d)) if d.has(b"Differences")),
                    _ => match encoding {
                        Some(Object::Name(_)) => true,
                        Some(Object::Dictionary(_)) => true,
                        _ => {
                            // Without an encoding: the standard fonts and fonts that are not symbolic.
                            let flags = f
                                .get(b"FontDescriptor")
                                .ok()
                                .and_then(|d| deref(doc, d).as_dict().ok())
                                .and_then(|d| d.get(b"Flags").ok())
                                .and_then(|v| v.as_i64().ok());
                            !matches!(flags, Some(fl) if fl & 4 != 0)
                        }
                    },
                };
                if !ok {
                    let name = f
                        .get(b"BaseFont")
                        .ok()
                        .and_then(|n| n.as_name().ok())
                        .map(|n| String::from_utf8_lossy(n).to_string())
                        .unwrap_or_else(|| "?".into());
                    out.push(CheckItem {
                        label: name,
                        page: Some(p),
                        ..CheckItem::default()
                    });
                }
            }
        }
        out
    }

    /// Check the document against every rule.
    pub fn accessibility_check(&self) -> Vec<CheckResult> {
        let tree = self.structure();
        let tagged = self.is_tagged() && tree.is_some();
        let pages = self.page_count();
        let mut results: HashMap<&'static str, (CheckStatus, Vec<CheckItem>)> = HashMap::new();
        let mut set = |rule: &'static str, status: CheckStatus, items: Vec<CheckItem>| {
            results.insert(rule, (status, items));
        };
        let failed_if = |items: Vec<CheckItem>| {
            if items.is_empty() {
                (CheckStatus::Passed, items)
            } else {
                (CheckStatus::Failed, items)
            }
        };
        let page_item = |p: usize| CheckItem {
            label: format!("{}", p + 1),
            page: Some(p),
            ..CheckItem::default()
        };

        // Document.
        let encrypt_p = self
            .inner
            .trailer
            .get(b"Encrypt")
            .ok()
            .and_then(|e| deref(&self.inner, e).as_dict().ok())
            .and_then(|e| e.get(b"P").ok())
            .and_then(|p| p.as_i64().ok());
        set(
            "permission",
            if matches!(encrypt_p, Some(p) if p & 512 == 0) {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            },
            Vec::new(),
        );
        let layers = self.text_layers();
        let has_text = layers.iter().any(|l| !l.glyphs.is_empty());
        let image_pages: Vec<CheckItem> = layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.glyphs.is_empty() && !l.images.is_empty())
            .map(|(p, _)| page_item(p))
            .collect();
        set(
            "image_only",
            if !has_text && !image_pages.is_empty() {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            },
            if has_text { Vec::new() } else { image_pages },
        );
        set(
            "tagged",
            if tagged {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            },
            Vec::new(),
        );
        set("logical_order", CheckStatus::Manual, Vec::new());
        set(
            "language",
            if self.language().is_empty() {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            },
            Vec::new(),
        );
        set(
            "title",
            if !self.title().is_empty() && self.display_doc_title() {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            },
            Vec::new(),
        );
        set(
            "bookmarks",
            if pages > 20 && self.outline().is_empty() {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            },
            Vec::new(),
        );
        set("contrast", CheckStatus::Manual, Vec::new());

        // Content.
        let mut owned_content: HashSet<(usize, i64)> = HashSet::new();
        let mut owned_annots: HashMap<(usize, usize), String> = HashMap::new();
        if let Some(t) = &tree {
            t.walk(&mut |e, _| {
                if e.is_root() {
                    return;
                }
                for k in &e.kids {
                    match k {
                        StructKid::Content { page, mcid } => {
                            owned_content.insert((*page, *mcid));
                        }
                        StructKid::Annot { page, index, .. } => {
                            owned_annots.insert((*page, *index), e.role.clone());
                        }
                        _ => {}
                    }
                }
            });
        }
        if tagged {
            let all: Vec<usize> = (0..pages).collect();
            let marked = self.marked_content_pages(&all);
            let mut items = Vec::new();
            for p in 0..pages {
                for m in marked.get(&p).map(|v| v.as_slice()).unwrap_or(&[]) {
                    let drawn = m.has_text || m.has_image || m.has_path;
                    let untagged = match m.mcid {
                        Some(id) => !owned_content.contains(&(p, id)),
                        None => !m.artifact,
                    };
                    if drawn && untagged && m.rect[2] > m.rect[0] {
                        items.push(CheckItem {
                            label: crate::structure::excerpt(&m.text, 60),
                            page: Some(p),
                            rect: Some(m.rect),
                            ..CheckItem::default()
                        });
                    }
                }
            }
            let (s, i) = failed_if(items);
            set("tagged_content", s, i);
        } else {
            set("tagged_content", CheckStatus::Failed, Vec::new());
        }
        let mut annot_items = Vec::new();
        let mut tab_items = Vec::new();
        let mut media_items = Vec::new();
        let mut field_items = Vec::new();
        let mut has_links = false;
        for p in 0..pages {
            let annots = self.annotations(p).unwrap_or_default();
            let size = self.page_size(p).ok();
            let mut page_has = false;
            for a in &annots {
                if !needs_tag(&a.subtype, a.hidden) {
                    continue;
                }
                page_has = true;
                has_links |= a.subtype == "Link";
                let rect = size
                    .as_ref()
                    .map(|s| crate::marks::pdf_rect_to_display(s, a.rect));
                let item = CheckItem {
                    label: a.subtype.clone(),
                    page: Some(p),
                    annot: Some((p, a.index)),
                    rect,
                    ..CheckItem::default()
                };
                if matches!(
                    a.subtype.as_str(),
                    "Screen" | "Movie" | "Sound" | "RichMedia" | "3D"
                ) {
                    media_items.push(item.clone());
                }
                match owned_annots.get(&(p, a.index)) {
                    None => {
                        if a.subtype == "Widget" {
                            field_items.push(item.clone());
                        }
                        annot_items.push(item);
                    }
                    Some(role) if a.subtype == "Widget" && role != "Form" => field_items.push(item),
                    _ => {}
                }
            }
            let tabs = self
                .page_id(p)
                .ok()
                .and_then(|pid| self.inner.get_dictionary(pid).ok())
                .and_then(|d| d.get(b"Tabs").ok())
                .and_then(|t| t.as_name().ok())
                .map(|t| t == b"S")
                .unwrap_or(false);
            if page_has && !tabs {
                tab_items.push(page_item(p));
            }
        }
        let (s, i) = failed_if(annot_items);
        set("tagged_annotations", s, i);
        let (s, i) = failed_if(tab_items);
        set("tab_order", s, i);
        let (s, i) = failed_if(self.fonts_without_unicode());
        set("encoding", s, i);
        let scripts = !self.scripts().is_empty();
        set(
            "multimedia",
            if media_items.is_empty() {
                CheckStatus::Passed
            } else {
                CheckStatus::Manual
            },
            media_items.clone(),
        );
        set(
            "flicker",
            if media_items.is_empty() && !scripts {
                CheckStatus::Passed
            } else {
                CheckStatus::Manual
            },
            Vec::new(),
        );
        set(
            "scripts",
            if scripts {
                CheckStatus::Manual
            } else {
                CheckStatus::Passed
            },
            Vec::new(),
        );
        set(
            "timed",
            if scripts {
                CheckStatus::Manual
            } else {
                CheckStatus::Passed
            },
            Vec::new(),
        );
        set(
            "navigation_links",
            if has_links {
                CheckStatus::Manual
            } else {
                CheckStatus::Passed
            },
            Vec::new(),
        );

        // Forms.
        let (s, i) = failed_if(field_items);
        set("tagged_fields", s, i);
        let mut desc = Vec::new();
        for f in self.form_fields() {
            let tu = self
                .inner
                .get_dictionary(f.id)
                .ok()
                .and_then(|d| dict_text(&self.inner, d, b"TU"))
                .unwrap_or_default();
            if tu.trim().is_empty() {
                desc.push(CheckItem {
                    label: f.name.clone(),
                    page: f.page,
                    ..CheckItem::default()
                });
            }
        }
        let (s, i) = failed_if(desc);
        set("field_descriptions", s, i);

        // Rules about the tags.
        let tag_rules = [
            "figures_alt",
            "nested_alt",
            "alt_content",
            "alt_hides_annot",
            "other_alt",
            "table_rows",
            "table_cells",
            "table_headers",
            "table_regular",
            "table_summary",
            "list_items",
            "list_lbl_body",
            "heading_nesting",
        ];
        match (&tree, tagged) {
            (Some(t), true) => {
                let elems = with_parents(t);
                let has_alt = |e: &StructElem| !e.alt.trim().is_empty() || !e.actual_text.trim().is_empty();
                let mut fig = Vec::new();
                let mut nested = Vec::new();
                let mut no_content = Vec::new();
                let mut hides = Vec::new();
                let mut other = Vec::new();
                let mut rows = Vec::new();
                let mut cells = Vec::new();
                let mut headers = Vec::new();
                let mut regular = Vec::new();
                let mut summary = Vec::new();
                let mut li = Vec::new();
                let mut lbl = Vec::new();
                let mut headings = Vec::new();
                let mut last_level = 0u8;
                for (e, parent) in &elems {
                    let role = e.role.as_str();
                    if role == "Figure" && !has_alt(e) {
                        fig.push(self.elem_item(e));
                    }
                    if !e.alt.trim().is_empty() {
                        let mut inner_alt = false;
                        for c in e.children() {
                            c.walk(&mut |d, _| inner_alt |= !d.alt.trim().is_empty());
                        }
                        if inner_alt {
                            nested.push(self.elem_item(e));
                        }
                        if e.is_empty() {
                            no_content.push(self.elem_item(e));
                        }
                        if !matches!(role, "Link" | "Form" | "Annot") && !e.annotations().is_empty() {
                            hides.push(self.elem_item(e));
                        }
                    }
                    if role == "Formula" && !has_alt(e) {
                        other.push(self.elem_item(e));
                    }
                    if role == "TR" && !matches!(parent.as_str(), "Table" | "THead" | "TBody" | "TFoot") {
                        rows.push(self.elem_item(e));
                    }
                    if matches!(role, "TH" | "TD") && parent != "TR" {
                        cells.push(self.elem_item(e));
                    }
                    if role == "Table" {
                        let mut th = false;
                        let mut spans = false;
                        let mut counts = Vec::new();
                        e.walk(&mut |d, _| {
                            th |= d.role == "TH";
                            spans |= d
                                .attributes
                                .iter()
                                .any(|a| a.ends_with("/ColSpan") || a.ends_with("/RowSpan"));
                            if d.role == "TR" {
                                counts.push(
                                    d.children()
                                        .filter(|c| matches!(c.role.as_str(), "TH" | "TD"))
                                        .count(),
                                );
                            }
                        });
                        if !th {
                            headers.push(self.elem_item(e));
                        }
                        if !spans && counts.windows(2).any(|w| w[0] != w[1]) {
                            regular.push(self.elem_item(e));
                        }
                        if !e.attributes.iter().any(|a| a.ends_with("/Summary")) && e.alt.trim().is_empty() {
                            summary.push(self.elem_item(e));
                        }
                    }
                    if role == "LI" && parent != "L" {
                        li.push(self.elem_item(e));
                    }
                    if matches!(role, "Lbl" | "LBody") && parent != "LI" {
                        lbl.push(self.elem_item(e));
                    }
                    if role != "H" {
                        if let Some(level) = heading_level(role) {
                            if level > last_level + 1 {
                                headings.push(self.elem_item(e));
                            }
                            last_level = level;
                        }
                    }
                }
                for (rule, items) in [
                    ("figures_alt", fig),
                    ("nested_alt", nested),
                    ("alt_content", no_content),
                    ("alt_hides_annot", hides),
                    ("other_alt", other),
                    ("table_rows", rows),
                    ("table_cells", cells),
                    ("table_headers", headers),
                    ("table_regular", regular),
                    ("table_summary", summary),
                    ("list_items", li),
                    ("list_lbl_body", lbl),
                    ("heading_nesting", headings),
                ] {
                    let (s, i) = failed_if(items);
                    set(rule, s, i);
                }
            }
            _ => {
                for rule in tag_rules {
                    set(rule, CheckStatus::Manual, Vec::new());
                }
            }
        }
        RULES
            .iter()
            .map(|r| {
                let (status, items) = results.remove(r.id).unwrap_or((CheckStatus::Manual, Vec::new()));
                CheckResult {
                    rule: r.id,
                    category: r.category,
                    status,
                    items,
                }
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Fixes
    // ------------------------------------------------------------------

    /// Set the primary language (`en-US`, `it-IT`...).
    pub fn set_language(&mut self, lang: &str) -> Result<()> {
        self.begin_edit();
        let cat = self.inner.trailer.get(b"Root")?.as_reference()?;
        let c = self.inner.get_dictionary_mut(cat)?;
        if lang.trim().is_empty() {
            c.remove(b"Lang");
        } else {
            c.set("Lang", lopdf::text_string(lang.trim()));
        }
        self.commit()
    }

    /// Give the document a title (the first heading, or `fallback`) when it
    /// has none, and show the title instead of the file name.
    pub fn fix_title(&mut self, fallback: &str) -> Result<()> {
        let title = if self.title().is_empty() {
            let heading = self
                .structure()
                .and_then(|t| {
                    let mut found = None;
                    t.walk(&mut |e, _| {
                        if found.is_none() && heading_level(&e.role).is_some() {
                            found = Some(e.clone());
                        }
                    });
                    found
                })
                .map(|e| self.struct_text(&e))
                .filter(|t| !t.trim().is_empty());
            heading.unwrap_or_else(|| fallback.to_string())
        } else {
            self.title()
        };
        let info = self.info();
        // One undo step: the viewer preference joins the step of the title.
        self.set_info(&title, &info.author, &info.subject, &info.keywords)?;
        let cat = self.inner.trailer.get(b"Root")?.as_reference()?;
        let c = self.inner.get_dictionary_mut(cat)?;
        let mut vp = match c.get(b"ViewerPreferences") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        vp.set("DisplayDocTitle", Object::Boolean(true));
        c.set("ViewerPreferences", Object::Dictionary(vp));
        self.commit()
    }

    /// Tab order following the structure on the pages with annotations;
    /// returns how many pages changed.
    pub fn set_tab_order_structure(&mut self) -> Result<usize> {
        let mut todo = Vec::new();
        for p in 0..self.page_count() {
            let has = self
                .annotations(p)
                .unwrap_or_default()
                .iter()
                .any(|a| needs_tag(&a.subtype, a.hidden));
            let pid = self.page_id(p)?;
            let tabs = self
                .inner
                .get_dictionary(pid)?
                .get(b"Tabs")
                .ok()
                .and_then(|t| t.as_name().ok())
                .map(|t| t == b"S")
                .unwrap_or(false);
            if has && !tabs {
                todo.push(pid);
            }
        }
        if todo.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        for pid in &todo {
            self.inner
                .get_dictionary_mut(*pid)?
                .set("Tabs", Object::Name(b"S".to_vec()));
        }
        self.commit()?;
        Ok(todo.len())
    }

    /// Descriptions (tooltips) made from the names of the fields that have
    /// none; returns how many fields changed.
    pub fn fill_field_descriptions(&mut self) -> Result<usize> {
        let todo: Vec<(ObjectId, String)> = self
            .form_fields()
            .into_iter()
            .filter(|f| {
                self.inner
                    .get_dictionary(f.id)
                    .ok()
                    .and_then(|d| dict_text(&self.inner, d, b"TU"))
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
            })
            .map(|f| (f.id, describe_name(&f.name)))
            .filter(|(_, d)| !d.is_empty())
            .collect();
        if todo.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        for (id, d) in &todo {
            self.inner
                .get_dictionary_mut(*id)?
                .set("TU", lopdf::text_string(d));
        }
        self.commit()?;
        Ok(todo.len())
    }

    /// Bookmarks from the headings of the structure (pages when there is no
    /// heading); returns how many bookmarks were made.
    pub fn bookmarks_from_structure(&mut self) -> Result<usize> {
        let mut heads: Vec<(u8, String, usize)> = Vec::new();
        if let Some(t) = self.structure() {
            let mut found = Vec::new();
            t.walk(&mut |e, _| {
                if let Some(level) = heading_level(&e.role) {
                    if let Some(p) = e.first_page() {
                        found.push((level, e.clone(), p));
                    }
                }
            });
            for (level, e, p) in found {
                let text = self.struct_text(&e);
                if !text.trim().is_empty() {
                    heads.push((level, crate::structure::excerpt(&text, 120), p));
                }
            }
        }
        if heads.is_empty() {
            self.bookmarks_from_pages()?;
            return Ok(self.page_count());
        }
        fn node_at<'a>(items: &'a mut [OutlineItem], path: &[usize]) -> Option<&'a mut OutlineItem> {
            let (first, rest) = path.split_first()?;
            let n = items.get_mut(*first)?;
            if rest.is_empty() {
                Some(n)
            } else {
                node_at(&mut n.children, rest)
            }
        }
        let mut items: Vec<OutlineItem> = Vec::new();
        // Open headings: level and path in the bookmark tree.
        let mut stack: Vec<(u8, Vec<usize>)> = Vec::new();
        for (level, title, page) in &heads {
            let item = self.new_bookmark_item(title, *page, None)?;
            while stack.last().is_some_and(|(l, _)| *l >= *level) {
                stack.pop();
            }
            let path = match stack
                .last()
                .and_then(|(_, p)| node_at(&mut items, p).map(|n| (n, p.clone())))
            {
                Some((parent, mut path)) => {
                    parent.children.push(item);
                    path.push(parent.children.len() - 1);
                    path
                }
                None => {
                    items.push(item);
                    vec![items.len() - 1]
                }
            };
            stack.push((*level, path));
        }
        self.set_outline(&items)?;
        Ok(heads.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_names_become_descriptions() {
        assert_eq!(describe_name("contact.firstName"), "First name");
        assert_eq!(describe_name("zip_code"), "Zip code");
        assert_eq!(describe_name("A"), "A");
    }
}

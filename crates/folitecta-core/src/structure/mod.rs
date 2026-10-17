//! Tagged PDF: the logical structure (tags) with its role map and class map,
//! the marked content of the pages and where it is drawn, automatic tagging,
//! tags made from an area of a page, and the reading order.
//!
//! Where content is drawn is measured with hayro: a device follows the marked
//! content sequences while the page is interpreted. Content that is not marked
//! yet is measured the same way after it is wrapped, in a copy of the
//! document, in probe sequences with their own identifiers.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, TransformExt, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{BezPath, Point, Rect, Shape};
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::content_lex::{Op, name_of, operations};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::geom;
use crate::util::{deref, dict_text};

/// Standard structure types.
pub const STANDARD_TYPES: &[&str] = &[
    "Document",
    "Part",
    "Art",
    "Sect",
    "Div",
    "BlockQuote",
    "Caption",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "H",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "P",
    "L",
    "LI",
    "Lbl",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Span",
    "Quote",
    "Note",
    "Reference",
    "BibEntry",
    "Code",
    "Link",
    "Annot",
    "Ruby",
    "RB",
    "RT",
    "RP",
    "Warichu",
    "WT",
    "WP",
    "Figure",
    "Formula",
    "Form",
];

/// Elements drawn inside a line of text: they belong to the block around them
/// in the reading order.
const INLINE: &[&str] = &[
    "Span",
    "Quote",
    "Note",
    "Reference",
    "BibEntry",
    "Code",
    "Link",
    "Annot",
    "Ruby",
    "RB",
    "RT",
    "RP",
    "Warichu",
    "WT",
    "WP",
    "Lbl",
];

/// Elements that only group others: never an item of the reading order by themselves.
const GROUPING: &[&str] = &[
    "Document",
    "Part",
    "Art",
    "Sect",
    "Div",
    "BlockQuote",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "TR",
    "THead",
    "TBody",
    "TFoot",
    "LI",
    "LBody",
];

/// Marked content identifiers of probe sequences (above any real one).
const PROBE_BASE: i64 = 900_000_000;
/// Most elements read from a structure tree.
const MAX_ELEMENTS: usize = 200_000;
/// Highest marked content identifier kept in the parent tree.
const MAX_MCID: i64 = 1_000_000;

pub fn is_standard(tag: &str) -> bool {
    STANDARD_TYPES.contains(&tag)
}

/// Heading level of a standard type (`H1` gives 1, `H` gives 1).
pub fn heading_level(role: &str) -> Option<u8> {
    match role {
        "H" => Some(1),
        _ => role
            .strip_prefix('H')
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=6).contains(n)),
    }
}

/// What an element of the structure contains, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum StructKid {
    Elem(StructElem),
    /// A marked content sequence of a page.
    Content {
        page: usize,
        mcid: i64,
    },
    /// An annotation (link, form field, comment): its page, index and subtype.
    Annot {
        page: usize,
        index: usize,
        subtype: String,
    },
    /// Something else: content of an XObject, an unknown reference.
    Other(String),
}

/// An element of the structure tree. The root has the tag `StructTreeRoot`.
#[derive(Debug, Clone, PartialEq)]
pub struct StructElem {
    pub id: ObjectId,
    /// The tag as written (`/S`).
    pub tag: String,
    /// The standard type the tag maps to through the role map.
    pub role: String,
    pub title: String,
    pub alt: String,
    pub actual_text: String,
    pub lang: String,
    /// Page of the element (its own `/Pg` or the one of an element around it).
    pub page: Option<usize>,
    /// The attribute owners and keys (`/A`), as `Owner/Key` strings.
    pub attributes: Vec<String>,
    pub kids: Vec<StructKid>,
}

impl StructElem {
    pub fn is_root(&self) -> bool {
        self.tag == "StructTreeRoot"
    }

    /// Child elements.
    pub fn children(&self) -> impl Iterator<Item = &StructElem> {
        self.kids.iter().filter_map(|k| match k {
            StructKid::Elem(e) => Some(e),
            _ => None,
        })
    }

    /// Every element of the subtree (this one first), with its depth.
    pub fn walk(&self, f: &mut dyn FnMut(&StructElem, usize)) {
        fn go(e: &StructElem, depth: usize, f: &mut dyn FnMut(&StructElem, usize)) {
            f(e, depth);
            for c in e.children() {
                go(c, depth + 1, f);
            }
        }
        go(self, 0, f);
    }

    pub fn find(&self, id: ObjectId) -> Option<&StructElem> {
        if self.id == id {
            return Some(self);
        }
        self.children().find_map(|c| c.find(id))
    }

    /// The element whose kids include `id`.
    pub fn parent_of(&self, id: ObjectId) -> Option<&StructElem> {
        if self.children().any(|c| c.id == id) {
            return Some(self);
        }
        self.children().find_map(|c| c.parent_of(id))
    }

    /// Marked content of the subtree, in order.
    pub fn content(&self) -> Vec<(usize, i64)> {
        let mut out = Vec::new();
        self.walk(&mut |e, _| {
            for k in &e.kids {
                if let StructKid::Content { page, mcid } = k {
                    out.push((*page, *mcid));
                }
            }
        });
        out
    }

    /// Annotations of the subtree, in order.
    pub fn annotations(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        self.walk(&mut |e, _| {
            for k in &e.kids {
                if let StructKid::Annot { page, index, .. } = k {
                    out.push((*page, *index));
                }
            }
        });
        out
    }

    /// The subtree has no content and no annotation.
    pub fn is_empty(&self) -> bool {
        self.content().is_empty() && self.annotations().is_empty()
    }

    /// First page with content of the subtree.
    pub fn first_page(&self) -> Option<usize> {
        self.content()
            .first()
            .map(|c| c.0)
            .or_else(|| self.annotations().first().map(|a| a.0))
            .or(self.page)
    }
}

/// Properties of an element that can be edited.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructProps {
    pub tag: String,
    pub title: String,
    pub alt: String,
    pub actual_text: String,
    pub lang: String,
}

/// Where a marked content sequence (or content outside them) is drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkedItem {
    /// None for artifacts and for content outside marked content.
    pub mcid: Option<i64>,
    pub tag: String,
    pub artifact: bool,
    /// Bounds in display space (points, y down).
    pub rect: [f32; 4],
    pub text: String,
    pub has_text: bool,
    pub has_image: bool,
    pub has_path: bool,
    /// Average size of the text.
    pub font_size: f32,
}

/// Structure tree read for a revision.
pub type StructCache = Option<(u64, Option<Arc<StructElem>>)>;

/// Marked content of pages measured for a revision.
pub type MarkedCache = Option<(u64, HashMap<usize, Arc<Vec<MarkedItem>>>)>;

/// One element of the reading order of a page.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderItem {
    pub id: ObjectId,
    pub tag: String,
    pub role: String,
    /// Bounds on the page (display space).
    pub rect: [f32; 4],
    pub text: String,
    /// Subtype of the first annotation of the item on the page (empty when none).
    pub annotation: String,
}

/// An item of the reading order being built: element, tag, role, bounds, texts.
type OrderAcc = (ObjectId, String, String, Option<[f32; 4]>, Vec<String>);

/// What automatic tagging made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutoTagReport {
    pub paragraphs: usize,
    pub headings: usize,
    pub figures: usize,
    pub artifacts: usize,
    pub annotations: usize,
}

mod content;
mod edit;
mod measure;
mod order;
mod read;
mod tagging;

use content::*;
use measure::*;
use read::*;

/// Short text of an element for lists (first words).
pub fn excerpt(text: &str, max: usize) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        t
    } else {
        format!("{}…", t.chars().take(max).collect::<String>().trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_wraps() {
        let data = b"q 0 0 m 5 5 l S Q BT /F1 12 Tf (a) Tj 0 -14 Td (b) Tj ET /P <</MCID 0>> BDC BT (c) Tj ET EMC /Im1 Do";
        let ops = operations(data);
        let units = content_units(data, &ops);
        let kinds: Vec<UnitKind> = units.iter().map(|u| u.kind).collect();
        assert_eq!(
            kinds,
            vec![UnitKind::Path, UnitKind::Text, UnitKind::Text, UnitKind::XObject]
        );
        assert_eq!(units[1].segment, units[2].segment);
        assert_eq!(Document::runs(&units[1..3]).len(), 1);
        assert_eq!(max_mcid(data), Some(0));
        let wraps = vec![(units[1].first, units[2].last, Wrap::Mcid("P".into(), 1))];
        let out = String::from_utf8(join(&write_wrapped(data, &ops, &wraps))).unwrap();
        assert!(out.contains("/P <</MCID 1>> BDC\n(a) Tj\n0 -14 Td\n(b) Tj\nEMC"));
        let stripped = String::from_utf8(join(&rewrite_mcids(data, &|_| McChange::Strip).unwrap())).unwrap();
        assert!(!stripped.contains("MCID") && !stripped.contains("EMC"));
        let art = String::from_utf8(join(&rewrite_mcids(data, &|_| McChange::Artifact).unwrap())).unwrap();
        assert!(art.contains("/Artifact BMC\nBT\n(c) Tj\nET\nEMC"));
    }

    #[test]
    fn attribute_text_round_trip() {
        let o = text_to_attrs("O=Layout Placement=Block SpaceBefore=6 BBox=[0 0 10 10]");
        let doc = lopdf::Document::with_version("1.7");
        let t = attrs_to_text(&doc, &o);
        assert!(t.contains("O=Layout") && t.contains("Placement=Block") && t.contains("SpaceBefore=6"));
        assert!(t.contains("BBox=[0 0 10 10]"));
        assert_eq!(pdf_name("My Tag"), "/My#20Tag");
        assert_eq!(heading_level("H3"), Some(3));
        assert_eq!(heading_level("Hx"), None);
    }
}

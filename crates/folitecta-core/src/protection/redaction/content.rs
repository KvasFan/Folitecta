//! What redaction reports, the covers drawn over the marks, and the comments
//! and form fields over the marks, which go with their replies.

use std::collections::HashSet;

use lopdf::{Object, ObjectId, dictionary};

use super::check::CheckData;
use super::holders::{Mark, PictureReason};
use super::overlay_text;
use super::sanitize::SanitizeReport;
use crate::document::{Document, PageSize};
use crate::error::Result;
use crate::util::{fmt_num, rect_of};

/// How marked areas are removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedactionMethod {
    /// The text, pictures and drawings under the marks are taken out of the
    /// page and of the drawings it reuses; the rest keeps its text and
    /// drawings. A page where that is not possible (a font Folitecta cannot
    /// edit, a gradient, pattern or mask that draws content under a mark,
    /// drawings nested too deeply) is drawn as a picture.
    #[default]
    Content,
    /// Every page with marks is drawn as a picture with the areas covered:
    /// nothing of the page stays text or drawing.
    Picture,
}

/// What applying redaction marks did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RedactionReport {
    /// Marked areas applied.
    pub areas: usize,
    /// Glyphs taken out of the content of the pages.
    pub glyphs: usize,
    /// Pictures whose pixels under the marks were covered.
    pub pictures: usize,
    /// Comments over the marks that were removed.
    pub comments: usize,
    /// Form fields with a widget over the marks that were removed (their values too).
    pub fields: usize,
    /// Pages drawn as pictures, by choice or because their content could not be edited.
    pub pages_as_pictures: Vec<usize>,
    /// Path objects and gradients whose geometry under the marks went.
    pub paths: usize,
    /// Reused drawings (forms) changed or copied for the redacted pages.
    pub forms: usize,
    /// Why each page in [`RedactionReport::pages_as_pictures`] is a picture.
    pub reasons: Vec<(usize, PictureReason)>,
    /// Areas blurred or pixelated: their pictures are only hidden from view.
    pub blurred_areas: usize,
    /// Digital signatures removed (a redacted document cannot keep them valid).
    pub signatures: usize,
    /// Type 3 fonts that lost glyphs under the marks and are still used
    /// elsewhere in a way that cannot be told glyph by glyph: their glyph
    /// procedures stay whole in the file, and may draw what the removed
    /// glyphs drew.
    pub type3_kept: usize,
    /// What sanitizing removed.
    pub sanitized: SanitizeReport,
    /// Every page edited by content passed the check before its covers
    /// were drawn (pages that failed became pictures).
    pub verified: bool,
    /// What the check of the saved file needs.
    pub(crate) check: CheckData,
}

impl RedactionReport {
    /// The terms of the removed text the check looks for (as written on the pages).
    pub fn terms(&self) -> Vec<String> {
        self.check.terms.iter().map(|t| t.shown().to_string()).collect()
    }
}

/// Two rectangles `[x0, y0, x1, y1]` overlap.
pub(super) fn overlap(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

/// The matrix from the page as shown (origin bottom-left, upright) to the
/// PDF user space of the page, for drawing on rotated pages.
fn upright_matrix(size: &PageSize) -> [f32; 6] {
    let c = size.crop_box;
    let (x0, y0, x1, y1) = (c[0], c[1], c[2], c[3]);
    match size.rotation {
        90 => [0.0, 1.0, -1.0, 0.0, x0 + size.height, y0],
        180 => [-1.0, 0.0, 0.0, -1.0, x1, y1],
        270 => [0.0, -1.0, 1.0, 0.0, x0, y1],
        _ => [1.0, 0.0, 0.0, 1.0, x0, y0],
    }
}

impl Document {
    /// Draw the covers of `marks` over `page`: the filled areas and their
    /// overlay texts, upright on rotated pages too, in a stream tagged `tag`
    /// (`/FolitectaMark`), which the check knows is not content.
    pub(super) fn draw_redaction_covers(&mut self, page: usize, marks: &[Mark], tag: &str) -> Result<()> {
        let id = self.page_id(page)?;
        let size = self.page_size_of(id);
        let m = upright_matrix(&size);
        let f = fmt_num;
        let mut body = format!(
            "q {} {} {} {} {} {} cm\n",
            f(m[0]),
            f(m[1]),
            f(m[2]),
            f(m[3]),
            f(m[4]),
            f(m[5])
        );
        let mut font = None;
        for m in marks {
            let (d, look) = (m.shown, &m.look);
            // Upright, origin at the bottom-left corner of the page as shown.
            let u = [d[0], size.height - d[3], d[2], size.height - d[1]];
            // A blurred or pixelated mark shows its pictures blurred, not a box.
            if look.overlay == super::Overlay::Standard {
                body.push_str(&format!(
                    "{} {} {} rg {} {} {} {} re f\n",
                    f(look.fill[0]),
                    f(look.fill[1]),
                    f(look.fill[2]),
                    f(u[0]),
                    f(u[1]),
                    f(u[2] - u[0]),
                    f(u[3] - u[1])
                ));
            }
            let text = overlay_text(look, u);
            if !text.is_empty() {
                font.get_or_insert_with(|| look.font.clone());
                body.push_str(&text);
            }
        }
        body.push_str("Q\n");
        if let Some(font) = font {
            let fid = self.inner.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => Object::Name(font.into_bytes()),
                "Encoding" => "WinAnsiEncoding",
            });
            self.add_page_resource(id, "Font", "FoliRed", Object::Reference(fid))?;
        }
        self.append_page_content(id, body.into_bytes(), Some(tag))?;
        Ok(())
    }

    /// Remove the comments of `page` over `marks` (PDF space) with their
    /// pop-up notes and replies, and the form fields with a widget over them,
    /// values included; the redaction marks themselves go too. Members of a
    /// group whose head goes stay, out of the group.
    pub(super) fn remove_annotations_over(
        &mut self,
        page: usize,
        marks: &[[f32; 4]],
        report: &mut RedactionReport,
    ) -> Result<()> {
        let id = self.page_id(page)?;
        let items = crate::annots::annot_refs(&self.inner, self.inner.get_dictionary(id)?);
        let mut roots: HashSet<ObjectId> = HashSet::new();
        let mut fields: Vec<ObjectId> = Vec::new();
        for (aid, d) in &items {
            let subtype = d
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                .unwrap_or_default();
            let rect = d.get(b"Rect").ok().and_then(|r| rect_of(&self.inner, r));
            let over = rect.is_some_and(|r| marks.iter().any(|m| overlap(&crate::geom::normalize(r), m)));
            if subtype == b"Redact" {
                roots.extend(*aid);
                continue;
            }
            // A pop-up note goes with its comment.
            if !over || subtype == b"Popup" {
                continue;
            }
            if subtype == b"Widget" {
                // The field holding the value: the widget itself or its parent.
                let field = match (
                    d.has(b"T"),
                    d.get(b"Parent").ok().and_then(|p| p.as_reference().ok()),
                ) {
                    (false, Some(p)) => Some(p),
                    _ => *aid,
                };
                if let Some(f) = field {
                    if !fields.contains(&f) {
                        fields.push(f);
                    }
                }
                continue;
            }
            report.comments += 1;
            roots.extend(*aid);
        }
        // Group members (replies of type Group) stay, without their head.
        for (aid, d) in &items {
            let Some(aid) = aid else { continue };
            let head = d.get(b"IRT").ok().and_then(|o| o.as_reference().ok());
            let group = d.get(b"RT").ok().and_then(|o| o.as_name().ok()) == Some(b"Group".as_slice());
            if group && head.is_some_and(|h| roots.contains(&h)) && !roots.contains(aid) {
                if let Ok(m) = self.inner.get_dictionary_mut(*aid) {
                    m.remove(b"IRT");
                    m.remove(b"RT");
                }
            }
        }
        // Replies and pop-ups of what goes go too.
        self.remove_annotations(id, &roots);
        for f in fields {
            self.remove_form_field(f)?;
            report.fields += 1;
        }
        Ok(())
    }
}

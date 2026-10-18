//! Redaction by content: the text and the pictures under the marks are taken
//! out of the page itself, so the rest of the page keeps its text and
//! drawings; the comments and form fields over the marks go too, and the
//! marked areas are covered as their marks say. A page whose content cannot
//! be edited that way is drawn as a picture instead (see [`super`]).

use hayro::kurbo::{Affine, Point};
use lopdf::{Object, ObjectId, dictionary};

use super::{RedactionLook, cover, overlay_text};
use crate::document::{Document, PageSize};
use crate::error::{Error, Result};
use crate::marks::pdf_rect_to_display;
use crate::util::{deref, fmt_num, rect_of};

/// How marked areas are removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedactionMethod {
    /// The text and the pictures under the marks are taken out of the page;
    /// the rest keeps its text and drawings. A page where that is not
    /// possible (text or pictures of a reused drawing, a picture written in
    /// the content, a font Folitecta cannot edit) is drawn as a picture.
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
    /// Take the text and pictures under `marks` (PDF space) out of `page`.
    /// Fails before changing anything when a picture under the marks is not
    /// one the page draws itself, or when the text cannot be edited.
    pub(super) fn redact_page_content(
        &mut self,
        page: usize,
        marks: &[([f32; 4], RedactionLook)],
        report: &mut RedactionReport,
    ) -> Result<()> {
        let id = self.page_id(page)?;
        let size = self.page_size_of(id);
        let shown: Vec<[f32; 4]> = marks
            .iter()
            .map(|(r, _)| pdf_rect_to_display(&size, *r))
            .collect();
        // Every picture drawn under a mark must be one of the page's own,
        // whose pixels can be changed.
        let images = self.page_images(page)?;
        let own: Vec<[f32; 4]> = images
            .iter()
            .map(|im| pdf_rect_to_display(&size, im.rect))
            .collect();
        for b in self.drawn_picture_bounds(page)? {
            if !shown.iter().any(|m| overlap(&b, m)) {
                continue;
            }
            let known = own.iter().any(|o| (0..4).all(|k| (o[k] - b[k]).abs() < 1.0));
            if !known {
                return Err(Error::Invalid(
                    "a picture under the marks is written in the content or inside a reused drawing".into(),
                ));
            }
        }
        report.glyphs += self.remove_text_in(page, &shown)?;
        for im in images.iter() {
            let m = Affine::new(im.matrix);
            if m.determinant().abs() < 1e-9 {
                continue;
            }
            let inv = m.inverse();
            let (w, h) = (im.width as f64, im.height as f64);
            let mut areas = Vec::new();
            for (r, look) in marks {
                if !overlap(r, &im.rect) {
                    continue;
                }
                // The corners of the mark in the pixels of the picture (top row first).
                let pts = [(r[0], r[1]), (r[2], r[1]), (r[0], r[3]), (r[2], r[3])]
                    .map(|(x, y)| inv * Point::new(x as f64, y as f64))
                    .map(|p| (p.x * w, (1.0 - p.y) * h));
                let x0 = pts
                    .iter()
                    .map(|p| p.0)
                    .fold(f64::MAX, f64::min)
                    .floor()
                    .clamp(0.0, w) as u32;
                let x1 = pts
                    .iter()
                    .map(|p| p.0)
                    .fold(f64::MIN, f64::max)
                    .ceil()
                    .clamp(0.0, w) as u32;
                let y0 = pts
                    .iter()
                    .map(|p| p.1)
                    .fold(f64::MAX, f64::min)
                    .floor()
                    .clamp(0.0, h) as u32;
                let y1 = pts
                    .iter()
                    .map(|p| p.1)
                    .fold(f64::MIN, f64::max)
                    .ceil()
                    .clamp(0.0, h) as u32;
                if x1 > x0 && y1 > y0 {
                    areas.push(([x0, y0, x1, y1], look.clone()));
                }
            }
            if areas.is_empty() {
                continue;
            }
            let mut pixels = self.image_pixels(page, im.index)?;
            for (r, look) in &areas {
                // Filled, or blurred and pixelated when the mark says so (the
                // look the user chose: it hides less than a filled area).
                cover(&mut pixels, *r, look);
            }
            self.put_page_image(page, im.index, &pixels, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], im.colors)?;
            report.pictures += 1;
        }
        Ok(())
    }

    /// Draw the covers of `marks` (PDF space) over `page`: the filled areas
    /// and their overlay texts, upright on rotated pages too.
    pub(super) fn draw_redaction_covers(
        &mut self,
        page: usize,
        marks: &[([f32; 4], RedactionLook)],
    ) -> Result<()> {
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
        for (r, look) in marks {
            let d = pdf_rect_to_display(&size, *r);
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
        self.append_page_content(id, body.into_bytes(), None)?;
        Ok(())
    }

    /// Remove the comments of `page` over `marks` (PDF space) and the form
    /// fields with a widget over them, values included; the redaction marks
    /// themselves go too.
    pub(super) fn remove_annotations_over(
        &mut self,
        page: usize,
        marks: &[[f32; 4]],
        report: &mut RedactionReport,
    ) -> Result<()> {
        let id = self.page_id(page)?;
        let annots: Vec<Object> = match self.inner.get_dictionary(id)?.get(b"Annots").ok() {
            Some(a) => deref(&self.inner, a).as_array().cloned().unwrap_or_default(),
            None => Vec::new(),
        };
        let mut keep = Vec::new();
        let mut gone: Vec<ObjectId> = Vec::new();
        let mut fields: Vec<ObjectId> = Vec::new();
        for a in annots {
            let aid = a.as_reference().ok();
            let Some(d) = deref(&self.inner, &a).as_dict().ok() else {
                keep.push(a);
                continue;
            };
            let subtype = d
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                .unwrap_or_default();
            let rect = d.get(b"Rect").ok().and_then(|r| rect_of(&self.inner, r));
            let over = rect.is_some_and(|r| marks.iter().any(|m| overlap(&crate::geom::normalize(r), m)));
            if subtype == b"Redact" {
                gone.extend(aid);
                continue;
            }
            // A pop-up note goes with its comment (below).
            if !over || subtype == b"Popup" {
                keep.push(a);
                continue;
            }
            if subtype == b"Widget" {
                // The field holding the value: the widget itself or its parent.
                let field = match (
                    d.has(b"T"),
                    d.get(b"Parent").ok().and_then(|p| p.as_reference().ok()),
                ) {
                    (false, Some(p)) => Some(p),
                    _ => aid,
                };
                if let Some(f) = field {
                    if !fields.contains(&f) {
                        fields.push(f);
                    }
                }
                keep.push(a);
                continue;
            }
            report.comments += 1;
            gone.extend(aid);
            if let Ok(p) = d.get(b"Popup").and_then(|p| p.as_reference()) {
                gone.push(p);
            }
        }
        // Pop-up notes of the comments that go.
        keep.retain(|a| a.as_reference().map(|r| !gone.contains(&r)).unwrap_or(true));
        self.inner
            .get_dictionary_mut(id)?
            .set("Annots", Object::Array(keep));
        for g in gone {
            self.inner.delete_object(g);
        }
        for f in fields {
            self.remove_form_field(f)?;
            report.fields += 1;
        }
        Ok(())
    }
}

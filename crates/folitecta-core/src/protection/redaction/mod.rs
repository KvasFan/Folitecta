//! Redaction: mark areas, then apply them. By default the content under the
//! marks is taken out of the pages and of the drawings they reuse — glyphs
//! leave their text operations, paths lose their geometry there, pictures and
//! inline pictures get their pixels covered (see [`holders`]) — and the rest
//! stays text and drawings; a page that cannot be edited that way — or every
//! page, when chosen — is drawn as a picture with the marked areas covered
//! (filled with a colour, blurred or pixelated). Comments and form fields
//! over the marks go with their replies, signatures are removed, an overlay
//! text (a redaction code, for example) is written over the areas, and the
//! document is sanitized as chosen ([`sanitize`]). The result is checked in
//! memory (a page that fails becomes a picture) and the bytes of a save can
//! be checked before they are written ([`check`]); the next save rewrites the
//! whole file, so nothing of the removed content stays in an earlier revision.

use std::collections::{BTreeMap, HashSet};

mod check;
mod content;
mod counts;
mod holders;
mod inline;
mod paths;
mod sanitize;
mod scan;
mod terms;
mod tidy;

pub use check::{CheckProblem, QuotePlace, RedactionCheck, check_redacted_bytes};
pub use content::{RedactionMethod, RedactionReport};
pub use holders::PictureReason;
pub use sanitize::{SANITIZE_BY_DEFAULT, SanitizeOptions, SanitizePart, SanitizeReport};
pub use terms::Term;

use hayro::kurbo::Rect;
use image::RgbaImage;
use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};
use rand_core::{OsRng, RngCore};

use self::check::{CheckData, CheckMark};
use self::counts::{Listing, drawing_counts, listed_by_page, purge, unlist};
use self::holders::{Mark, Work};
use crate::document::Document;
use crate::error::{Error, Result};
use crate::marks::pdf_rect_to_display;
use crate::page_text::probe::{ProbeOptions, probe_pages};
use crate::render::{RenderOpts, Renderer};
use crate::util::{deref, fmt_num, helvetica_width, rect_of, winansi_byte};

/// How a redacted area is covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overlay {
    /// Filled with a colour.
    #[default]
    Standard,
    Blur,
    BlurExtra,
    Pixelate,
    PixelateExtra,
}

impl Overlay {
    pub const ALL: [Overlay; 5] = [
        Overlay::Standard,
        Overlay::Blur,
        Overlay::BlurExtra,
        Overlay::Pixelate,
        Overlay::PixelateExtra,
    ];

    fn key(self) -> Option<&'static str> {
        match self {
            Overlay::Standard => None,
            Overlay::Blur => Some("Blur"),
            Overlay::BlurExtra => Some("BlurExtra"),
            Overlay::Pixelate => Some("Pixelate"),
            Overlay::PixelateExtra => Some("PixelateExtra"),
        }
    }

    fn from_key(k: &[u8]) -> Overlay {
        match k {
            b"Blur" => Overlay::Blur,
            b"BlurExtra" => Overlay::BlurExtra,
            b"Pixelate" => Overlay::Pixelate,
            b"PixelateExtra" => Overlay::PixelateExtra,
            _ => Overlay::Standard,
        }
    }
}

/// Standard fonts of overlay texts: name and the resource name used in /DA.
pub const OVERLAY_FONTS: &[(&str, &str)] = &[
    ("Helvetica", "Helv"),
    ("Helvetica-Bold", "HeBo"),
    ("Times-Roman", "TiRo"),
    ("Times-Bold", "TiBo"),
    ("Courier", "Cour"),
    ("Courier-Bold", "CoBo"),
];

/// How a redacted area looks once applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionLook {
    pub overlay: Overlay,
    /// Colour of a standard cover.
    pub fill: [f32; 3],
    /// Text written over the area (a redaction code, for example).
    pub text: String,
    /// One of [`OVERLAY_FONTS`].
    pub font: String,
    /// Font size; 0 makes the text fit the area.
    pub font_size: f32,
    pub text_color: [f32; 3],
    /// The text fills the area, repeated.
    pub repeat: bool,
}

impl Default for RedactionLook {
    fn default() -> Self {
        RedactionLook {
            overlay: Overlay::Standard,
            fill: [0.0, 0.0, 0.0],
            text: String::new(),
            font: "Helvetica".into(),
            font_size: 0.0,
            text_color: [1.0, 1.0, 1.0],
            repeat: false,
        }
    }
}

fn color_of(o: Option<&Object>) -> Option<[f32; 3]> {
    let a = o?.as_array().ok()?;
    let v: Vec<f32> = a
        .iter()
        .filter_map(|x| x.as_float().ok().or_else(|| x.as_i64().ok().map(|i| i as f32)))
        .collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn color_obj(c: [f32; 3]) -> Object {
    Object::Array(c.iter().map(|v| Object::Real(*v)).collect())
}

/// The look written on a redaction mark.
fn write_look(d: &mut Dictionary, look: &RedactionLook) {
    d.set("IC", color_obj(look.fill));
    match look.overlay.key() {
        Some(k) => d.set("FoliOverlay", Object::Name(k.as_bytes().to_vec())),
        None => {
            d.remove(b"FoliOverlay");
        }
    }
    if look.text.is_empty() {
        d.remove(b"OverlayText");
    } else {
        d.set("OverlayText", lopdf::text_string(&look.text));
    }
    d.set("Repeat", look.repeat);
    let res = OVERLAY_FONTS
        .iter()
        .find(|(n, _)| *n == look.font)
        .map(|(_, r)| *r)
        .unwrap_or("Helv");
    let c = look.text_color;
    d.set(
        "DA",
        Object::string_literal(format!(
            "/{res} {} Tf {} {} {} rg",
            fmt_num(look.font_size.max(0.0)),
            fmt_num(c[0]),
            fmt_num(c[1]),
            fmt_num(c[2])
        )),
    );
}

/// The look read from a redaction mark.
fn read_look(doc: &lopdf::Document, d: &Dictionary) -> RedactionLook {
    let mut look = RedactionLook {
        fill: color_of(d.get(b"IC").ok().map(|o| deref(doc, o))).unwrap_or([0.0, 0.0, 0.0]),
        overlay: d
            .get(b"FoliOverlay")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(Overlay::from_key)
            .unwrap_or_default(),
        repeat: d
            .get(b"Repeat")
            .ok()
            .and_then(|o| o.as_bool().ok())
            .unwrap_or(false),
        text: crate::util::dict_text(doc, d, b"OverlayText").unwrap_or_default(),
        ..RedactionLook::default()
    };
    if let Some(da) = d.get(b"DA").ok().and_then(|o| o.as_str().ok()) {
        let da = String::from_utf8_lossy(da).into_owned();
        let toks: Vec<&str> = da.split_whitespace().collect();
        for (i, t) in toks.iter().enumerate() {
            if *t == "Tf" && i >= 2 {
                let res = toks[i - 2].trim_start_matches('/');
                if let Some((name, _)) = OVERLAY_FONTS.iter().find(|(_, r)| *r == res) {
                    look.font = name.to_string();
                }
                look.font_size = toks[i - 1].parse().unwrap_or(0.0);
            }
            if *t == "rg" && i >= 3 {
                let v: Vec<f32> = toks[i - 3..i].iter().filter_map(|x| x.parse().ok()).collect();
                if v.len() == 3 {
                    look.text_color = [v[0], v[1], v[2]];
                }
            }
        }
    }
    look
}

/// Width of a text at 1 point in a standard font.
fn text_width(font: &str, text: &str) -> f32 {
    let n = text.chars().count() as f32;
    let w = if font.starts_with("Courier") {
        0.6 * n
    } else if font.starts_with("Times") {
        helvetica_width(text, 1.0) * 0.92
    } else {
        helvetica_width(text, 1.0)
    };
    if font.ends_with("Bold") { w * 1.06 } else { w }
}

/// Content drawing an overlay text inside `r` (PDF space of the drawn page).
fn overlay_text(look: &RedactionLook, r: [f32; 4]) -> String {
    let text = look.text.trim();
    let (w, h) = (r[2] - r[0], r[3] - r[1]);
    if text.is_empty() || w < 1.0 || h < 1.0 {
        return String::new();
    }
    let unit = text_width(&look.font, text).max(0.1);
    let size = if look.font_size > 0.0 {
        look.font_size
    } else if look.repeat {
        (h * 0.7).clamp(4.0, 12.0)
    } else {
        (h * 0.7).min(w * 0.92 / unit).max(2.0)
    };
    let bytes: Vec<u8> = text.chars().map(winansi_byte).collect();
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let f = fmt_num;
    let c = look.text_color;
    let mut out = format!(
        "q {} {} {} {} re W n BT /FoliRed {} Tf {} {} {} rg ",
        f(r[0]),
        f(r[1]),
        f(w),
        f(h),
        f(size),
        f(c[0]),
        f(c[1]),
        f(c[2])
    );
    let line_w = unit * size;
    if look.repeat {
        // Rows of the text from the top, as many as fit.
        let gap = size * 0.6;
        let step_x = line_w + gap;
        let step_y = size * 1.25;
        let mut y = r[3] - size;
        let mut row = 0;
        while y > r[1] - size && row < 400 {
            // Rows shift by half a word, like a pattern.
            let mut x = r[0] - if row % 2 == 1 { step_x / 2.0 } else { 0.0 };
            while x < r[2] {
                out.push_str(&format!("1 0 0 1 {} {} Tm <{hex}> Tj ", f(x), f(y)));
                x += step_x;
            }
            y -= step_y;
            row += 1;
        }
    } else {
        let x = r[0] + (w - line_w) / 2.0;
        let y = r[1] + (h - size * 0.7) / 2.0;
        out.push_str(&format!("1 0 0 1 {} {} Tm <{hex}> Tj ", f(x), f(y)));
    }
    out.push_str("ET Q\n");
    out
}

/// Cover the pixels `[x0, y0, x1, y1)` of a drawn page as the look says.
fn cover(img: &mut RgbaImage, r: [u32; 4], look: &RedactionLook) {
    let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    match look.overlay {
        Overlay::Standard => {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let px = image::Rgba([q(look.fill[0]), q(look.fill[1]), q(look.fill[2]), 255]);
            for y in y0..y1 {
                for x in x0..x1 {
                    img.put_pixel(x, y, px);
                }
            }
        }
        Overlay::Blur | Overlay::BlurExtra => {
            let sigma = if look.overlay == Overlay::Blur { 8.0 } else { 18.0 };
            let part = image::imageops::crop_imm(img, x0, y0, w, h).to_image();
            let blurred = image::imageops::blur(&part, sigma);
            image::imageops::replace(img, &blurred, x0 as i64, y0 as i64);
        }
        Overlay::Pixelate | Overlay::PixelateExtra => {
            let block = if look.overlay == Overlay::Pixelate { 12 } else { 26 };
            let mut by = y0;
            while by < y1 {
                let mut bx = x0;
                let bh = block.min(y1 - by);
                while bx < x1 {
                    let bw = block.min(x1 - bx);
                    let mut sum = [0u64; 3];
                    for y in by..by + bh {
                        for x in bx..bx + bw {
                            let p = img.get_pixel(x, y);
                            for c in 0..3 {
                                sum[c] += p[c] as u64;
                            }
                        }
                    }
                    let n = (bw * bh) as u64;
                    let avg = image::Rgba([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255]);
                    for y in by..by + bh {
                        for x in bx..bx + bw {
                            img.put_pixel(x, y, avg);
                        }
                    }
                    bx += bw;
                }
                by += bh;
            }
        }
    }
}

/// How redaction marks are applied.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionOptions {
    pub method: RedactionMethod,
    pub sanitize: SanitizeOptions,
}

impl Default for RedactionOptions {
    fn default() -> Self {
        RedactionOptions {
            method: RedactionMethod::Content,
            sanitize: SanitizeOptions::default(),
        }
    }
}

/// What applying redaction marks involves (shown before applying them).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RedactionSummary {
    pub areas: usize,
    pub pages: usize,
    /// Areas blurred or pixelated instead of filled.
    pub blurred: usize,
    /// Digital signatures the redaction removes.
    pub signatures: usize,
}

impl Document {
    /// Mark a rectangle for redaction (a `Redact` annotation with a red outline).
    pub fn add_redaction(&mut self, page: usize, rect: [f32; 4], author: &str) -> Result<ObjectId> {
        self.add_redaction_with(page, rect, author, &RedactionLook::default())
    }

    /// Mark a rectangle for redaction with the look it takes once applied.
    pub fn add_redaction_with(
        &mut self,
        page: usize,
        rect: [f32; 4],
        author: &str,
        look: &RedactionLook,
    ) -> Result<ObjectId> {
        let page_id = self.page_id(page)?;
        self.begin_edit();
        let mut annot = self.base_annotation(page_id, "Redact", rect, author, "", [1.0, 0.0, 0.0]);
        write_look(&mut annot, look);
        self.finish_annotation(page_id, annot)
    }

    /// The look of the redaction mark `index` of a page (None when it is not one).
    pub fn redaction_look(&self, page: usize, index: usize) -> Option<RedactionLook> {
        let id = self.annotations(page).ok()?.get(index)?.id?;
        let d = self.inner.get_dictionary(id).ok()?;
        (d.get(b"Subtype").ok()?.as_name().ok()? == b"Redact").then(|| read_look(&self.inner, d))
    }

    /// Change the look of a redaction mark.
    pub fn set_redaction_look(&mut self, page: usize, index: usize, look: &RedactionLook) -> Result<()> {
        let id = self
            .annotations(page)?
            .get(index)
            .and_then(|a| a.id)
            .ok_or_else(|| Error::Invalid("the comment does not exist".into()))?;
        let is_redact = self
            .inner
            .get_dictionary(id)?
            .get(b"Subtype")
            .ok()
            .and_then(|s| s.as_name().ok())
            == Some(b"Redact".as_slice());
        if !is_redact {
            return Err(Error::Invalid("the comment is not a redaction mark".into()));
        }
        self.begin_edit();
        write_look(self.inner.get_dictionary_mut(id)?, look);
        self.commit()
    }

    /// Pending redaction marks as `(page, rect)`.
    pub fn pending_redactions(&self) -> Vec<(usize, [f32; 4])> {
        (0..self.page_count())
            .filter_map(|p| self.annotations(p).ok())
            .flatten()
            .filter(|a| a.subtype == "Redact")
            .map(|a| (a.page, a.rect))
            .collect()
    }

    /// Apply the pending redaction marks of the given pages (all pages when
    /// `None`), taking the content under them out of the pages
    /// ([`RedactionMethod::Content`]). Returns the number of areas.
    pub fn apply_redactions(&mut self, pages: Option<&[usize]>) -> Result<usize> {
        Ok(self.apply_redactions_with(pages, RedactionMethod::Content)?.areas)
    }

    /// Apply the pending redaction marks of the given pages (all pages when
    /// `None`) with `method`, as one undo step, without sanitizing (see
    /// [`Document::apply_redactions_opts`]).
    pub fn apply_redactions_with(
        &mut self,
        pages: Option<&[usize]>,
        method: RedactionMethod,
    ) -> Result<RedactionReport> {
        self.apply_redactions_opts(
            pages,
            &RedactionOptions {
                method,
                sanitize: SanitizeOptions::none(),
            },
        )
    }

    /// The pending redaction marks of the given pages (all when `None`): page,
    /// rectangle and look.
    pub fn pending_redaction_marks(&self, pages: Option<&[usize]>) -> Vec<(usize, [f32; 4], RedactionLook)> {
        let mut out = Vec::new();
        for p in 0..self.page_count() {
            if pages.is_some_and(|sel| !sel.contains(&p)) {
                continue;
            }
            for a in self.annotations(p).unwrap_or_default() {
                if a.subtype != "Redact" {
                    continue;
                }
                let look = self.redaction_look(p, a.index).unwrap_or_default();
                out.push((p, crate::geom::normalize(a.rect), look));
            }
        }
        out
    }

    /// What applying the marks of `pages` (all when `None`) would involve.
    pub fn redaction_summary(&self, pages: Option<&[usize]>) -> RedactionSummary {
        let marks = self.pending_redaction_marks(pages);
        let mut on: Vec<usize> = marks.iter().map(|(p, _, _)| *p).collect();
        on.dedup();
        RedactionSummary {
            areas: marks.len(),
            pages: on.len(),
            blurred: marks
                .iter()
                .filter(|(_, _, l)| l.overlay != Overlay::Standard)
                .count(),
            signatures: self.signed_field_count(),
        }
    }

    /// The words of the text under the marks of `pages` (all when `None`),
    /// as sanitizing and the check look for them.
    pub fn redaction_terms(&self, pages: Option<&[usize]>, placeholder: &str) -> Vec<Term> {
        let marks = self.marks_by_page(pages);
        self.terms_of(&marks, None, placeholder)
    }

    /// What applying the marks of `pages` would remove besides their content,
    /// with the terms found under the marks (a preview: nothing changes; it
    /// draws the marked pages, so call it away from the interface thread).
    pub fn redaction_preview(
        &self,
        pages: Option<&[usize]>,
        opts: &SanitizeOptions,
    ) -> (Vec<Term>, SanitizeReport) {
        let terms = self.redaction_terms(pages, &opts.placeholder);
        let report = self.sanitize_preview(opts, &terms);
        (terms, report)
    }

    fn marks_by_page(&self, pages: Option<&[usize]>) -> BTreeMap<usize, Vec<Mark>> {
        let mut by_page: BTreeMap<usize, Vec<Mark>> = BTreeMap::new();
        for (p, r, look) in self.pending_redaction_marks(pages) {
            let Ok(size) = self.page_size(p) else { continue };
            by_page.entry(p).or_default().push(Mark {
                pdf: Rect::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64),
                shown: pdf_rect_to_display(&size, r),
                look,
            });
        }
        by_page
    }

    /// The terms of the glyphs under the marks (from `probe` when given).
    fn terms_of(
        &self,
        marks: &BTreeMap<usize, Vec<Mark>>,
        probe: Option<&crate::page_text::probe::Probe>,
        placeholder: &str,
    ) -> Vec<Term> {
        let pages: Vec<usize> = marks.keys().copied().collect();
        let own;
        let probe = match probe {
            Some(p) => Some(p),
            None => {
                own = probe_pages(
                    &self.inner,
                    &pages,
                    ProbeOptions {
                        reveal: true,
                        forms: true,
                    },
                    None,
                )
                .ok();
                own.as_ref()
            }
        };
        let mut out: Vec<Term> = Vec::new();
        for (p, ms) in marks {
            let Some(pp) = probe.and_then(|pr| pr.pages.get(p)) else {
                continue;
            };
            let rects: Vec<([f32; 4], String)> = ms
                .iter()
                .map(|m| {
                    let code = m.look.text.trim();
                    (
                        m.shown,
                        if code.is_empty() {
                            placeholder.to_string()
                        } else {
                            code.to_string()
                        },
                    )
                })
                .collect();
            for t in terms::terms_of_page(&pp.glyphs, &rects) {
                if !out.iter().any(|o| o.text == t.text) {
                    out.push(t);
                }
            }
        }
        out
    }

    /// Apply the pending redaction marks of the given pages (all pages when
    /// `None`) as one undo step: the content under the marks is removed
    /// ([`RedactionOptions::method`]), the comments and form fields over them
    /// go with their replies, signatures are removed, the areas are covered
    /// as their marks say, the marks go and the document is sanitized as
    /// [`RedactionOptions::sanitize`] says. Pages are checked before their
    /// covers are drawn; a page that fails is drawn as a picture. The next
    /// save writes the whole file again, so no earlier revision keeps the
    /// removed content.
    pub fn apply_redactions_opts(
        &mut self,
        pages: Option<&[usize]>,
        opts: &RedactionOptions,
    ) -> Result<RedactionReport> {
        let marks = self.marks_by_page(pages);
        if marks.is_empty() {
            return Ok(RedactionReport::default());
        }
        self.begin_edit();
        match self.apply_marks(marks, opts) {
            Ok(r) => Ok(r),
            Err(e) => {
                self.abandon_edit();
                Err(e)
            }
        }
    }

    fn apply_marks(
        &mut self,
        marks: BTreeMap<usize, Vec<Mark>>,
        opts: &RedactionOptions,
    ) -> Result<RedactionReport> {
        let mut report = RedactionReport {
            areas: marks.values().map(Vec::len).sum(),
            blurred_areas: marks
                .values()
                .flatten()
                .filter(|m| m.look.overlay != Overlay::Standard)
                .count(),
            verified: true,
            ..RedactionReport::default()
        };
        // The covers of this redaction, told apart from content by a tag of their own.
        let mut nonce = [0u8; 8];
        OsRng.fill_bytes(&mut nonce);
        let cover_tag = format!(
            "Redaction{}",
            nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        report.signatures = self.clear_signatures_inner();
        if report.signatures > 0 {
            self.mark_signature_fields();
        }
        let pages: Vec<usize> = marks.keys().copied().collect();
        let probe = probe_pages(
            &self.inner,
            &pages,
            ProbeOptions {
                reveal: true,
                forms: true,
            },
            None,
        )
        .ok();
        // Every word under the marks is looked for by the check; the ones the
        // person chose to leave are not replaced where they are quoted.
        let all_terms = self.terms_of(&marks, probe.as_ref(), &opts.sanitize.placeholder);
        let terms: Vec<Term> = all_terms
            .iter()
            .filter(|t| {
                !opts
                    .sanitize
                    .skip_terms
                    .contains(&t.text.iter().collect::<String>())
            })
            .cloned()
            .collect();
        let counts = drawing_counts(&self.inner);
        // The pictures, forms and patterns the marked pages list in their
        // resources (shared, or inherited from the page tree): those nothing
        // draws once the redaction is done leave those resources.
        let listed: Vec<Listing> = {
            let ids = self.page_ids();
            marks
                .keys()
                .filter_map(|p| ids.get(*p))
                .flat_map(|pid| listed_by_page(&self.inner, *pid))
                .collect()
        };
        let markers = probe.as_ref().map(|p| p.ids.draw_markers()).unwrap_or_default();
        let mut work = Work::default();
        let mut as_pictures: BTreeMap<usize, PictureReason> = BTreeMap::new();
        for (&p, ms) in &marks {
            let result = match (&probe, opts.method) {
                (_, RedactionMethod::Picture) => Err(PictureReason::Chosen),
                (None, _) => Err(PictureReason::Content),
                (Some(pr), RedactionMethod::Content) => {
                    self.redact_page(p, ms, pr, &markers, &counts, &mut work)
                }
            };
            if let Err(reason) = result {
                as_pictures.insert(p, reason);
            }
        }
        // Patterns of paints removed whole that hold what was under the
        // marks cannot stay where something else on their page still draws
        // them.
        let page_ids = self.page_ids();
        for (pid, pat, reason) in &work.patterns {
            if let Some(p) = page_ids.iter().position(|x| x == pid) {
                if !as_pictures.contains_key(&p) && self.drawn_by_page(*pid).contains(pat) {
                    as_pictures.insert(p, *reason);
                }
            }
        }
        // Comments and fields over the marks go either way, with the marks.
        for (&p, ms) in &marks {
            let rects: Vec<[f32; 4]> = ms
                .iter()
                .map(|m| [m.pdf.x0 as f32, m.pdf.y0 as f32, m.pdf.x1 as f32, m.pdf.y1 as f32])
                .collect();
            self.remove_annotations_over(p, &rects, &mut report)?;
        }
        let check_marks: BTreeMap<usize, Vec<CheckMark>> = marks
            .iter()
            .map(|(p, ms)| {
                (
                    *p,
                    ms.iter()
                        .map(|m| CheckMark::new(m.shown, m.look.overlay, m.look.fill))
                        .collect(),
                )
            })
            .collect();
        // Pages edited are checked before their covers are drawn.
        let edited: BTreeMap<usize, Vec<CheckMark>> = check_marks
            .iter()
            .filter(|(p, _)| !as_pictures.contains_key(p))
            .map(|(p, m)| (*p, m.clone()))
            .collect();
        if !edited.is_empty() {
            let (problems, _) = self.check_redacted_pages(&edited, None);
            for (p, _) in problems {
                as_pictures.entry(p).or_insert(PictureReason::Check);
                report.verified = false;
            }
        }
        for (&p, ms) in &marks {
            if !as_pictures.contains_key(&p) {
                self.draw_redaction_covers(p, ms, &cover_tag)?;
            }
        }
        // Pages drawn as pictures no longer draw what they drew (Type 3 fonts
        // included).
        let mut type3_lost = work.type3_lost.clone();
        let mut replaced: Vec<(ObjectId, usize)> = Vec::new();
        let mut picture_pages: HashSet<ObjectId> = HashSet::new();
        for &p in as_pictures.keys() {
            let Some(&pid) = page_ids.get(p) else { continue };
            picture_pages.insert(pid);
            type3_lost.extend(self.type3_fonts_of(pid));
            for id in self.drawn_by_page(pid) {
                replaced.push((id, p));
            }
        }
        if !as_pictures.is_empty() {
            self.commit()?;
            let by_page: BTreeMap<usize, Vec<Mark>> = marks
                .iter()
                .filter(|(p, _)| as_pictures.contains_key(p))
                .map(|(p, m)| (*p, m.clone()))
                .collect();
            self.draw_pages_as_pictures(by_page, &cover_tag)?;
        }
        report.pages_as_pictures = as_pictures.keys().copied().collect();
        report.reasons = as_pictures.iter().map(|(p, r)| (*p, *r)).collect();
        // Originals replaced on the pages: they go when nothing draws them.
        for id in &work.replaced {
            let page = work
                .copies
                .iter()
                .find(|(o, _, _)| o == id)
                .and_then(|(_, _, pg)| page_ids.iter().position(|x| x == pg))
                .unwrap_or_else(|| marks.keys().next().copied().unwrap_or(0));
            replaced.push((*id, page));
        }
        // Page thumbnails, metadata and the private data of other programs
        // describe the old page.
        for &p in marks.keys() {
            if let Some(&pid) = page_ids.get(p) {
                if let Ok(d) = self.inner.get_dictionary_mut(pid) {
                    d.remove(b"Thumb");
                    d.remove(b"PieceInfo");
                    d.remove(b"LastModified");
                    d.remove(b"Metadata");
                }
            }
        }
        // An XFA form repeats the page text and the field values.
        self.remove_xfa()?;
        report.glyphs = work.glyphs;
        report.pictures = work.pictures;
        report.paths = work.paths;
        report.forms = work.forms;
        report.sanitized = self.sanitize_in(&opts.sanitize, &terms);
        // What nothing draws any more goes with every reference to it — in
        // resources shared with other pages or inherited from the page tree,
        // in the structure tree: originals replaced by copies, drawings of
        // removed layers and comments.
        let counts_after = drawing_counts(&self.inner);
        let drawn_after = |id: &ObjectId| counts_after.get(id).copied().unwrap_or(0) > 0;
        let gone: HashSet<ObjectId> = replaced
            .iter()
            .map(|(id, _)| *id)
            .chain(counts.map.iter().filter(|(_, n)| **n > 0).map(|(id, _)| *id))
            .filter(|id| !drawn_after(id))
            .collect();
        // What the marked pages listed and nothing ever drew leaves their
        // resources only: it goes with them unless something else (the mask
        // of a picture, say) holds it.
        for l in &listed {
            if !gone.contains(&l.id) && !drawn_after(&l.id) {
                unlist(&mut self.inner, l);
            }
        }
        self.tidy_structure(
            &work.copies,
            &work.touched,
            &picture_pages,
            &gone,
            &terms,
            &opts.sanitize.placeholder,
        );
        purge(&mut self.inner, &gone);
        self.inner.prune_objects();
        // Type 3 fonts that lost glyphs and are still drawn where the codes
        // shown cannot be told keep all their procedures.
        report.type3_kept = type3_lost
            .iter()
            .filter(|id| self.inner.objects.contains_key(id) && counts_after.type3_unknown.contains(id))
            .count();
        self.commit()?;
        // The file opened is no base for an incremental update any more: it
        // still holds what was removed.
        self.source = None;
        report.check = CheckData {
            marks: check_marks,
            terms: all_terms,
            skipped: opts.sanitize.skip_terms.clone(),
            quoting: opts.sanitize.quoting,
            cover_tag: cover_tag.into_bytes(),
            replaced,
        };
        Ok(report)
    }

    /// The form says signature fields exist (ISO 32000-1 §12.7.2, Table 219:
    /// bit 1) when empty ones remain after the signatures went.
    fn mark_signature_fields(&mut self) {
        let any_field = self
            .form_fields()
            .iter()
            .any(|f| f.kind == crate::forms::FieldKind::Signature);
        if !any_field {
            return;
        }
        let af = self
            .inner
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .cloned();
        match af {
            Some(Object::Reference(r)) => {
                if let Ok(d) = self.inner.get_dictionary_mut(r) {
                    d.set("SigFlags", 1i64);
                }
            }
            Some(Object::Dictionary(_)) => {
                if let Ok(Object::Dictionary(d)) =
                    self.inner.catalog_mut().and_then(|c| c.get_mut(b"AcroForm"))
                {
                    d.set("SigFlags", 1i64);
                }
            }
            _ => {}
        }
    }

    /// The Type 3 fonts a page lists in its resources and in those of the
    /// forms and patterns it lists.
    fn type3_fonts_of(&self, page_id: ObjectId) -> Vec<ObjectId> {
        let doc = &self.inner;
        let mut all = vec![crate::page_text::probe::page_resources(doc, page_id)];
        for l in listed_by_page(doc, page_id) {
            if let Some(r) = doc
                .get_object(l.id)
                .and_then(Object::as_stream)
                .ok()
                .and_then(|s| s.dict.get(b"Resources").ok())
                .and_then(|r| deref(doc, r).as_dict().ok())
            {
                all.push(r.clone());
            }
        }
        let mut out = Vec::new();
        for r in &all {
            let Some(fonts) = r.get(b"Font").ok().and_then(|f| deref(doc, f).as_dict().ok()) else {
                continue;
            };
            for (k, v) in fonts.iter() {
                if let (Ok(id), Some(_)) = (v.as_reference(), counts::type3_font(doc, r, k)) {
                    out.push(id);
                }
            }
        }
        out
    }

    /// The pictures, forms and patterns a page draws (forms and the cells of
    /// patterns followed).
    fn drawn_by_page(&self, page_id: ObjectId) -> Vec<ObjectId> {
        use crate::page_text::probe::{stream_data, stream_resources};
        let doc = &self.inner;
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let res = crate::page_text::probe::page_resources(doc, page_id);
        let data = crate::content_edit::joined(doc, page_id).data;
        let mut todo = vec![(data, res)];
        while let Some((data, res)) = todo.pop() {
            for op in crate::content_lex::operations(&data) {
                let (cat, name): (&[u8], _) = match op.operator {
                    b"Do" => (b"XObject", op.operands.first()),
                    b"scn" | b"SCN" => (b"Pattern", op.operands.last()),
                    _ => continue,
                };
                let Some(id) = name
                    .and_then(|r| crate::content_lex::name_of(&data, *r))
                    .and_then(|n| {
                        res.get(cat)
                            .ok()
                            .and_then(|x| deref(doc, x).as_dict().ok())
                            .and_then(|x| x.get(&n).ok())
                            .and_then(|v| v.as_reference().ok())
                    })
                else {
                    continue;
                };
                if !seen.insert(id) {
                    continue;
                }
                out.push(id);
                if let Ok(s) = doc.get_object(id).and_then(Object::as_stream) {
                    let image = s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok())
                        == Some(b"Image".as_slice());
                    if !image {
                        todo.push((stream_data(s), stream_resources(doc, s, &res)));
                    }
                }
            }
        }
        out
    }

    /// Draw every page of `by_page` as a picture at 200 dpi, with the marked
    /// areas covered as their marks say and their overlay texts over them (in
    /// a stream tagged `cover_tag`, which is not content).
    fn draw_pages_as_pictures(&mut self, by_page: BTreeMap<usize, Vec<Mark>>, cover_tag: &str) -> Result<()> {
        let renderer = Renderer::new(self.bytes())?;
        let scale = 200.0 / 72.0;
        for (p, marks) in by_page {
            let id = self.page_id(p)?;
            let size = self.page_size_of(id);
            let opts = RenderOpts {
                annotations: false,
                transparent: false,
            };
            let mut img = renderer.render_rgba(p, scale, opts)?;
            let (iw, ih) = img.dimensions();
            let (sx, sy) = (iw as f32 / size.width, ih as f32 / size.height);
            let mut texts = String::new();
            for m in &marks {
                // Display space (y down) is the space of the drawn page.
                let d = m.shown;
                let px = [
                    ((d[0] * sx).floor().max(0.0) as u32).min(iw),
                    ((d[1] * sy).floor().max(0.0) as u32).min(ih),
                    ((d[2] * sx).ceil().max(0.0) as u32).min(iw),
                    ((d[3] * sy).ceil().max(0.0) as u32).min(ih),
                ];
                cover(&mut img, px, &m.look);
                // The page becomes upright and starts at the origin.
                let r = [d[0], size.height - d[3], d[2], size.height - d[1]];
                texts.push_str(&overlay_text(&m.look, r));
            }
            let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
            let mut jpeg = std::io::Cursor::new(Vec::new());
            rgb.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88))
                .map_err(|e| Error::Image(e.to_string()))?;
            let (w, h) = rgb.dimensions();
            let xid = self.inner.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => w as i64,
                    "Height" => h as i64,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8i64,
                    "Filter" => "DCTDecode",
                },
                jpeg.into_inner(),
            ));
            let f = fmt_num;
            let content = format!(
                "q {} 0 0 {} 0 0 cm /FoliRaster Do Q\n",
                f(size.width),
                f(size.height)
            );
            let resources = dictionary! { "XObject" => dictionary! { "FoliRaster" => xid } };
            {
                let page = self.inner.get_dictionary_mut(id)?;
                page.set(
                    "MediaBox",
                    Object::Array(vec![0.into(), 0.into(), size.width.into(), size.height.into()]),
                );
                page.remove(b"CropBox");
                page.remove(b"Rotate");
                page.set("Resources", Object::Dictionary(resources));
                if size.rotation != 0 {
                    page.remove(b"Annots");
                }
            }
            self.inner.change_page_content(id, content.into_bytes())?;
            if !texts.is_empty() {
                let font = marks
                    .iter()
                    .find(|m| !m.look.text.trim().is_empty())
                    .map(|m| m.look.font.clone())
                    .unwrap_or_else(|| "Helvetica".into());
                let fid = self.inner.add_object(dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => Object::Name(font.into_bytes()),
                    "Encoding" => "WinAnsiEncoding",
                });
                self.add_page_resource(id, "Font", "FoliRed", Object::Reference(fid))?;
                self.append_page_content(id, texts.into_bytes(), Some(cover_tag))?;
            }
        }
        Ok(())
    }

    /// Rectangles of redaction marks on a page (for display).
    pub fn redaction_rects(&self, page: usize) -> Vec<[f32; 4]> {
        let Ok(page_id) = self.page_id(page) else {
            return Vec::new();
        };
        let Ok(dict) = self.inner.get_dictionary(page_id) else {
            return Vec::new();
        };
        dict.get(b"Annots")
            .ok()
            .map(|a| deref(&self.inner, a))
            .and_then(|a| a.as_array().ok())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| deref(&self.inner, a).as_dict().ok())
                    .filter(|d| d.get(b"Subtype").ok().and_then(|s| s.as_name().ok()) == Some(b"Redact"))
                    .filter_map(|d| d.get(b"Rect").ok().and_then(|r| rect_of(&self.inner, r)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_are_written_and_read_back() {
        let look = RedactionLook {
            overlay: Overlay::PixelateExtra,
            fill: [0.2, 0.4, 0.6],
            text: "(b)(6)".into(),
            font: "Courier-Bold".into(),
            font_size: 9.0,
            text_color: [1.0, 0.0, 0.0],
            repeat: true,
        };
        let mut d = Dictionary::new();
        write_look(&mut d, &look);
        let doc = lopdf::Document::with_version("1.7");
        let back = read_look(&doc, &d);
        assert_eq!(back.overlay, look.overlay);
        assert_eq!(back.text, look.text);
        assert_eq!(back.font, look.font);
        assert_eq!(back.font_size, 9.0);
        assert!(back.repeat);
        assert!((back.fill[1] - 0.4).abs() < 1e-4 && (back.text_color[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn covers() {
        let mut img = RgbaImage::from_fn(40, 40, |x, _| image::Rgba([(x * 6) as u8, 0, 0, 255]));
        let solid = RedactionLook {
            fill: [0.0, 0.0, 1.0],
            ..Default::default()
        };
        cover(&mut img, [0, 0, 10, 10], &solid);
        assert_eq!(img.get_pixel(5, 5).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(15, 5)[0], 90);
        let pix = RedactionLook {
            overlay: Overlay::Pixelate,
            ..Default::default()
        };
        cover(&mut img, [12, 0, 24, 12], &pix);
        // One block: every pixel the same.
        assert_eq!(img.get_pixel(12, 0), img.get_pixel(23, 11));
        let text = overlay_text(
            &RedactionLook {
                text: "CODE".into(),
                ..Default::default()
            },
            [10.0, 10.0, 110.0, 30.0],
        );
        assert!(
            text.contains("re W n") && text.contains("<434F4445> Tj"),
            "{text}"
        );
    }
}

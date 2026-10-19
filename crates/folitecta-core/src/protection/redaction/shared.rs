//! What the marked pages share with other pages: patterns painted under the
//! marks that other pages still paint with. Such a pattern stays in the file
//! for them; that is harmless where those pages show what it holds (a whole
//! cell of a tiling pattern, the colours of a gradient under the marks) in a
//! filled rectangle under rectangular clips. Otherwise the pattern still
//! holds what was under the marks, and the report says so.

use std::collections::{HashMap, HashSet};

use hayro::kurbo::{Affine, Point, Rect};
use lopdf::{Dictionary, Object, ObjectId};

use super::counts::Counts;
use super::holders::{
    PatternKind, affine_of, axis_aligned, cell_tiles, pattern_kind_of, rect_value, shading_type,
};
use super::paths::{exclusion_clip, shading_shown_elsewhere};
use super::scan::{Gs, Paint, exact_region, page_state, path_bounds, scan};
use crate::content_edit::joined;
use crate::document::Document;
use crate::page_text::probe::{page_resources, stream_data, stream_resources};
use crate::util::deref;

/// A paint with a pattern: the pattern, the matrix from pattern space to
/// page space, the bounds of what it paints and, for a filled rectangle under
/// rectangular clips, exactly what it paints (page space).
#[derive(Debug, Clone)]
pub(super) struct PatternPaint {
    pub id: ObjectId,
    pub m: Affine,
    pub area: Rect,
    pub exact: Option<Rect>,
}

/// A pattern painted under the marks of a page: the page, the paint and the
/// parts of the marks it paints.
#[derive(Debug, Clone)]
pub(super) struct UnderMarks {
    pub page: ObjectId,
    pub paint: PatternPaint,
    pub under: Vec<Rect>,
}

impl Document {
    /// The paints with patterns of a page (the forms it draws followed).
    pub(super) fn pattern_paints(&self, page_id: ObjectId) -> Vec<PatternPaint> {
        let c = self.page_size_of(page_id).crop_box;
        let gs0 = page_state(Rect::new(c[0] as f64, c[1] as f64, c[2] as f64, c[3] as f64));
        let res = page_resources(&self.inner, page_id);
        let data = joined(&self.inner, page_id).data;
        let mut out = Vec::new();
        self.paints_of(&data, &res, &gs0, 0, &mut Vec::new(), &mut out);
        out
    }

    fn paints_of(
        &self,
        data: &[u8],
        res: &Dictionary,
        gs0: &Gs,
        depth: usize,
        stack: &mut Vec<ObjectId>,
        out: &mut Vec<PatternPaint>,
    ) {
        let doc = &self.inner;
        let sc = scan(doc, data, res, gs0);
        let pattern = |name: &[u8]| -> Option<(ObjectId, Affine)> {
            let v = res
                .get(b"Pattern")
                .ok()
                .and_then(|p| deref(doc, p).as_dict().ok())?
                .get(name)
                .ok()?;
            let id = v.as_reference().ok()?;
            let dict = match doc.get_object(id).ok()? {
                Object::Dictionary(d) => d,
                Object::Stream(s) => &s.dict,
                _ => return None,
            };
            // Pattern space is the default space of the stream painting.
            Some((id, gs0.ctm * affine_of(doc, dict.get(b"Matrix").ok())))
        };
        for p in &sc.paths {
            let area = path_bounds(p)
                .map(|b| b.intersect(p.gs.clip))
                .unwrap_or(Rect::ZERO);
            let mut used: Vec<(&Option<Vec<u8>>, Option<Rect>)> = Vec::new();
            if matches!(p.paint, Paint::Fill(_) | Paint::FillStroke(_)) {
                used.push((&p.gs.fill_pattern, exact_region(p)));
            }
            if matches!(p.paint, Paint::Stroke | Paint::FillStroke(_)) {
                used.push((&p.gs.stroke_pattern, None));
            }
            for (name, exact) in used {
                if let Some((id, m)) = name.as_deref().filter(|n| !n.is_empty()).and_then(pattern) {
                    out.push(PatternPaint { id, m, area, exact });
                }
            }
        }
        if depth >= 12 {
            return;
        }
        for d in &sc.forms {
            if stack.contains(&d.id) {
                continue;
            }
            let Ok(fs) = doc.get_object(d.id).and_then(Object::as_stream) else {
                continue;
            };
            let bbox = rect_value(doc, fs.dict.get(b"BBox").ok()).unwrap_or(Rect::new(-1e6, -1e6, 1e6, 1e6));
            let m = d.gs.ctm * affine_of(doc, fs.dict.get(b"Matrix").ok());
            let mut gs1 = d.gs.clone();
            gs1.ctm = m;
            gs1.clip = d.gs.clip.intersect(m.transform_rect_bbox(bbox));
            gs1.clip_exact &= axis_aligned(m);
            let r = stream_resources(doc, fs, res);
            stack.push(d.id);
            self.paints_of(&stream_data(fs), &r, &gs1, depth + 1, stack, out);
            stack.pop();
        }
    }

    /// The patterns the marked pages paint under their marks (`marks`: page
    /// object, marks in page space).
    pub(super) fn patterns_under_marks(&self, marks: &[(ObjectId, Vec<Rect>)]) -> Vec<UnderMarks> {
        let mut out = Vec::new();
        for (page, rects) in marks {
            for paint in self.pattern_paints(*page) {
                let under: Vec<Rect> = rects
                    .iter()
                    .map(|r| r.intersect(paint.area))
                    .filter(|r| r.width() > 0.0 && r.height() > 0.0)
                    .collect();
                if !under.is_empty() {
                    out.push(UnderMarks {
                        page: *page,
                        paint,
                        under,
                    });
                }
            }
        }
        out
    }

    /// How many patterns painted under the marks (`under`, found before the
    /// redaction) other pages still paint with, without showing what they
    /// hold: they stay in the file for those pages. `marks` are the marks of
    /// every redacted page and `counts` the drawings after the redaction.
    pub(super) fn patterns_kept_for_others(
        &self,
        under: &[UnderMarks],
        marks: &[(ObjectId, Vec<Rect>)],
        counts: &Counts,
    ) -> usize {
        let doc = &self.inner;
        // Patterns still drawn, but no longer by the page that painted them
        // under its marks (a page that still does showed them elsewhere, or
        // became a picture).
        let mut left: Vec<&UnderMarks> = Vec::new();
        let mut drawn_by: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
        for u in under {
            if !doc.objects.contains_key(&u.paint.id) || counts.get(&u.paint.id).copied().unwrap_or(0) == 0 {
                continue;
            }
            let on_page = drawn_by
                .entry(u.page)
                .or_insert_with(|| self.pattern_paints(u.page).into_iter().map(|p| p.id).collect());
            if !on_page.contains(&u.paint.id) {
                left.push(u);
            }
        }
        if left.is_empty() {
            return 0;
        }
        // The paints of those patterns on every page.
        let wanted: HashSet<ObjectId> = left.iter().map(|u| u.paint.id).collect();
        let marks_of: HashMap<ObjectId, &Vec<Rect>> = marks.iter().map(|(p, r)| (*p, r)).collect();
        let mut elsewhere: HashMap<ObjectId, Vec<(ObjectId, PatternPaint)>> = HashMap::new();
        for page in self.page_ids() {
            for paint in self.pattern_paints(page) {
                if wanted.contains(&paint.id) {
                    elsewhere.entry(paint.id).or_default().push((page, paint));
                }
            }
        }
        let mut kept: HashSet<ObjectId> = HashSet::new();
        for u in left {
            let others: Vec<(&PatternPaint, &[Rect])> = elsewhere
                .get(&u.paint.id)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter(|(page, _)| *page != u.page)
                .map(|(page, p)| (p, marks_of.get(page).map(|r| r.as_slice()).unwrap_or_default()))
                .collect();
            if !self.shown_by_others(u, &others) {
                kept.insert(u.paint.id);
            }
        }
        kept.len()
    }

    /// Whether other pages' paints (with the marks of their pages) show
    /// what the pattern of `u` painted under the marks.
    fn shown_by_others(&self, u: &UnderMarks, others: &[(&PatternPaint, &[Rect])]) -> bool {
        let doc = &self.inner;
        let Ok(p) = doc.get_object(u.paint.id) else {
            return true;
        };
        match pattern_kind_of(doc, p, &Dictionary::new()) {
            PatternKind::Tiling { cell: None, .. } => true,
            PatternKind::Tiling {
                cell: Some(c), step, ..
            } => others
                .iter()
                .any(|(o, rects)| o.exact.is_some_and(|e| cell_tiles(c, step, o.m, e, rects).1)),
            PatternKind::Shading(sh, _) => {
                let kind = shading_type(doc, &sh);
                if kind != 2 && kind != 3 {
                    return false;
                }
                // In the space of the shading (pattern space).
                let Some(under) = invert(u.paint.m).map(|to| {
                    u.under
                        .iter()
                        .map(|r| to.transform_rect_bbox(*r))
                        .collect::<Vec<Rect>>()
                }) else {
                    return false;
                };
                let outside: Vec<Vec<Point>> = others
                    .iter()
                    .filter_map(|(o, rects)| {
                        let to = invert(o.m)?;
                        let (_, pieces) = exclusion_clip(o.exact?, rects, Affine::IDENTITY)?;
                        Some(
                            pieces
                                .into_iter()
                                .map(|piece| piece.into_iter().map(|q| to * q).collect::<Vec<Point>>())
                                .collect::<Vec<_>>(),
                        )
                    })
                    .flatten()
                    .collect();
                !outside.is_empty() && shading_shown_elsewhere(doc, &sh, Affine::IDENTITY, &under, &outside)
            }
            PatternKind::Unknown => false,
        }
    }
}

fn invert(m: Affine) -> Option<Affine> {
    (m.determinant().abs() > 1e-12).then(|| m.inverse())
}

//! Where marked content is drawn: a device follows the marked content sequences while hayro interprets a page.

use super::*;

// ----------------------------------------------------------------------
// Measuring marked content
// ----------------------------------------------------------------------

#[derive(Default)]
pub(super) struct Acc {
    tag: String,
    rect: Option<Rect>,
    text: String,
    last_end: Option<(Point, f64)>,
    size_sum: f64,
    glyphs: usize,
    image: bool,
    path: bool,
}

impl Acc {
    fn add_rect(&mut self, r: Rect) {
        if !r.width().is_finite() || !r.height().is_finite() {
            return;
        }
        self.rect = Some(match self.rect {
            Some(a) => a.union(r),
            None => r,
        });
    }
}

/// Key of the content outside marked content and of artifacts.
pub(super) const UNMARKED: i64 = i64::MIN;
pub(super) const ARTIFACT: i64 = i64::MIN + 1;

#[derive(Default)]
pub(super) struct McDevice {
    stack: Vec<(Vec<u8>, Option<i32>)>,
    order: Vec<i64>,
    items: HashMap<i64, Acc>,
    /// Positions of the last filled glyph run (fill and stroke text arrives twice).
    last_fill: Vec<(i64, i64)>,
}

impl McDevice {
    fn current(&mut self) -> &mut Acc {
        let (key, tag) = match self
            .stack
            .iter()
            .rev()
            .find_map(|(t, m)| m.map(|m| (m as i64, t.clone())))
        {
            Some((m, t)) => (m, t),
            None if self.stack.iter().any(|(t, _)| t == b"Artifact") => (ARTIFACT, b"Artifact".to_vec()),
            None => (UNMARKED, Vec::new()),
        };
        if !self.items.contains_key(&key) {
            self.order.push(key);
        }
        self.items.entry(key).or_insert_with(|| Acc {
            tag: String::from_utf8_lossy(&tag).to_string(),
            ..Acc::default()
        })
    }

    fn into_items(mut self) -> Vec<(i64, Acc)> {
        self.order
            .iter()
            .filter_map(|k| self.items.remove(k).map(|a| (*k, a)))
            .collect()
    }
}

impl<'a> Device<'a> for McDevice {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, _mode: &DrawMode) {
        let r = (props.transform * path.clone()).bounding_box();
        let acc = self.current();
        acc.path = true;
        acc.add_rect(r);
    }
    fn push_clip_path(&mut self, _clip_path: &ClipPath) {}
    fn push_transparency_group(
        &mut self,
        _opacity: f32,
        _mask: Option<SoftMask<'a>>,
        _blend_mode: BlendMode,
    ) {
    }
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        let glyphs = run.glyphs();
        let key: Vec<(i64, i64)> = glyphs
            .iter()
            .map(|g| {
                let o = props.transform * g.transform() * Point::ZERO;
                ((o.x * 10.0) as i64, (o.y * 10.0) as i64)
            })
            .collect();
        if matches!(mode, DrawMode::Stroke(_)) && key == self.last_fill {
            return;
        }
        if !matches!(mode, DrawMode::Stroke(_)) {
            self.last_fill = key;
        }
        for (i, g) in glyphs.iter().enumerate() {
            let t = props.transform * g.transform();
            let (adv, text) = match &**g {
                Glyph::Outline(o) => (o.advance_width().map(|a| a as f64), g.as_unicode()),
                Glyph::Type3(_) => (None, g.as_unicode()),
            };
            let adv = adv.filter(|a| *a > 0.0).unwrap_or_else(|| {
                glyphs
                    .get(i + 1)
                    .map(|n| (g.transform().inverse() * n.transform() * Point::ZERO).x)
                    .filter(|d| d.is_finite() && *d > 0.0 && *d < 3000.0)
                    .unwrap_or(500.0)
            });
            let text = match text {
                Some(hayro::hayro_interpret::hayro_cmap::BfString::Char(c)) => c.to_string(),
                Some(hayro::hayro_interpret::hayro_cmap::BfString::String(s)) => s,
                None => String::new(),
            };
            let origin = t * Point::ZERO;
            let size = ((t * Point::new(0.0, 1000.0)) - origin).hypot();
            if !size.is_finite() || size < 0.01 {
                continue;
            }
            let end = t * Point::new(adv, 0.0);
            let r = Rect::from_points(t * Point::new(0.0, 800.0), t * Point::new(adv, 800.0))
                .union_pt(t * Point::new(0.0, -200.0))
                .union_pt(t * Point::new(adv, -200.0));
            let acc = self.current();
            if let Some((last, last_size)) = acc.last_end {
                let s = last_size.max(size);
                let dy = (origin.y - last.y).abs();
                let gap = origin.x - last.x;
                if (dy > 0.5 * s || gap > 0.25 * s || gap < -1.5 * s)
                    && !acc.text.ends_with(' ')
                    && !text.starts_with(char::is_whitespace)
                {
                    acc.text.push(' ');
                }
            }
            for c in text.chars() {
                acc.text
                    .push(if c.is_control() || c == '\u{a0}' { ' ' } else { c });
            }
            acc.last_end = Some((end, size));
            acc.size_sum += size;
            acc.glyphs += 1;
            acc.add_rect(r);
        }
    }
    fn draw_image(&mut self, _image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let unit = Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1);
        let r = (props.transform * unit).bounding_box();
        let acc = self.current();
        acc.image = true;
        acc.add_rect(r);
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>) {
        self.stack.push((tag.to_vec(), mcid));
    }
    fn end_marked_content(&mut self) {
        self.stack.pop();
    }
}

pub(super) fn to_item(key: i64, a: Acc) -> MarkedItem {
    let rect = a
        .rect
        .map(|r| [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32])
        .unwrap_or_default();
    MarkedItem {
        mcid: (key != UNMARKED && key != ARTIFACT).then_some(key),
        tag: a.tag,
        artifact: key == ARTIFACT,
        rect,
        text: a.text.split_whitespace().collect::<Vec<_>>().join(" "),
        has_text: a.glyphs > 0,
        has_image: a.image,
        has_path: a.path,
        font_size: if a.glyphs > 0 {
            (a.size_sum / a.glyphs as f64) as f32
        } else {
            0.0
        },
    }
}

/// Measure the marked content of some pages of a serialized document.
pub(super) fn measure(bytes: Arc<Vec<u8>>, pages: &[usize]) -> Result<HashMap<usize, Vec<MarkedItem>>> {
    let pdf = Pdf::new(bytes).map_err(|e| Error::Render(format!("cannot parse document: {e:?}")))?;
    let all = pdf.pages();
    let mut out = HashMap::new();
    for &p in pages {
        let Some(page) = all.get(p) else { continue };
        let (w, h) = page.render_dimensions();
        let cache = InterpreterCache::new();
        let settings = InterpreterSettings {
            render_annotations: false,
            ..InterpreterSettings::default()
        };
        let mut ctx = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, w as f64, h as f64),
            &cache,
            page.xref(),
            settings,
        );
        let mut dev = McDevice::default();
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            interpret_page(page, &mut ctx, &mut dev);
        }))
        .is_ok();
        let items = if ok {
            dev.into_items().into_iter().map(|(k, a)| to_item(k, a)).collect()
        } else {
            Vec::new()
        };
        out.insert(p, items);
    }
    Ok(out)
}

//! More page tools: crop to content, duplicate detection, splitting pages into
//! tiles, overlaying another document, transitions, backgrounds, page labels.

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::render::{RenderOpts, Renderer};
use crate::textops::display_rect_to_pdf;
use crate::util::{deref, fmt_num, num, rect_of};

/// Sides affected by "crop to white margins".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sides {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl Sides {
    pub const ALL: Sides = Sides {
        left: true,
        top: true,
        right: true,
        bottom: true,
    };
}

/// Page transition styles (PDF `/Trans /S`).
pub const TRANSITIONS: &[&str] = &[
    "R", "Split", "Blinds", "Box", "Wipe", "Dissolve", "Glitter", "Fly", "Push", "Cover", "Uncover", "Fade",
];

impl Document {
    pub(crate) fn check(&self, pages: &[usize]) -> Result<()> {
        let n = self.page_count();
        match pages.iter().find(|p| **p >= n) {
            Some(p) => Err(Error::InvalidPage(*p)),
            None => Ok(()),
        }
    }

    /// Display-space bounds of the non-white pixels of a page (`None` for a blank page).
    pub fn content_bounds(&self, page: usize) -> Result<Option<[f32; 4]>> {
        let renderer = Renderer::new(self.bytes())?;
        let scale = 1.0;
        let opts = RenderOpts {
            annotations: false,
            transparent: false,
        };
        let px = renderer.render_rgba(page, scale, opts)?;
        let (w, h) = px.dimensions();
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for (x, y, p) in px.enumerate_pixels() {
            if p[0] < 240 || p[1] < 240 || p[2] < 240 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        if x0 == u32::MAX || w == 0 || h == 0 {
            return Ok(None);
        }
        Ok(Some([
            x0 as f32 / scale,
            y0 as f32 / scale,
            x1 as f32 / scale,
            y1 as f32 / scale,
        ]))
    }

    /// Crop pages to their visible content plus `margin` points on the chosen sides.
    /// Blank pages are left unchanged. Returns how many pages were cropped.
    pub fn crop_to_white_margins(&mut self, pages: &[usize], margin: f32, sides: Sides) -> Result<usize> {
        self.check(pages)?;
        let mut boxes = Vec::new();
        for &p in pages {
            if let Some(b) = self.content_bounds(p)? {
                boxes.push((p, b));
            }
        }
        if boxes.is_empty() {
            return Ok(0);
        }
        self.begin_edit();
        let ids = self.page_ids();
        for (p, b) in &boxes {
            let size = self.page_size_of(ids[*p]);
            let mut d = [0.0, 0.0, size.width, size.height];
            if sides.left {
                d[0] = (b[0] - margin).max(0.0);
            }
            if sides.top {
                d[1] = (b[1] - margin).max(0.0);
            }
            if sides.right {
                d[2] = (b[2] + margin).min(size.width);
            }
            if sides.bottom {
                d[3] = (b[3] + margin).min(size.height);
            }
            let r = display_rect_to_pdf(&size, d);
            self.inner
                .get_dictionary_mut(ids[*p])?
                .set("CropBox", Object::Array(r.iter().map(|v| (*v).into()).collect()));
        }
        self.commit()?;
        Ok(boxes.len())
    }

    /// Groups (size >= 2) of pages that look identical.
    pub fn duplicate_page_groups(&self) -> Result<Vec<Vec<usize>>> {
        let renderer = Renderer::new(self.render_bytes()?)?;
        let texts = self.all_text();
        let mut sigs: Vec<(u64, (u32, u32))> = Vec::new();
        for p in 0..self.page_count() {
            let gray =
                image::DynamicImage::ImageRgba8(renderer.render_rgba(p, 0.25, RenderOpts::default())?)
                    .to_luma8();
            let dims = gray.dimensions();
            let small = image::imageops::resize(&gray, 9, 8, image::imageops::FilterType::Triangle);
            let mut hash = 0u64;
            for y in 0..8 {
                for x in 0..8 {
                    hash <<= 1;
                    if small.get_pixel(x, y)[0] > small.get_pixel(x + 1, y)[0] {
                        hash |= 1;
                    }
                }
            }
            sigs.push((hash, dims));
        }
        let mut used = vec![false; sigs.len()];
        let mut groups = Vec::new();
        for i in 0..sigs.len() {
            if used[i] {
                continue;
            }
            let mut g = vec![i];
            for j in i + 1..sigs.len() {
                if !used[j]
                    && sigs[i].1 == sigs[j].1
                    && (sigs[i].0 ^ sigs[j].0).count_ones() <= 2
                    && texts[i] == texts[j]
                {
                    used[j] = true;
                    g.push(j);
                }
            }
            if g.len() > 1 {
                groups.push(g);
            }
        }
        Ok(groups)
    }

    /// Split each page into `cols` x `rows` pages (reading order), using crop boxes.
    pub fn split_pages_grid(&mut self, pages: &[usize], cols: usize, rows: usize) -> Result<()> {
        self.check(pages)?;
        let (cols, rows) = (cols.clamp(1, 20), rows.clamp(1, 20));
        if cols * rows == 1 {
            return Ok(());
        }
        self.begin_edit();
        let ids = self.page_ids();
        self.inherit_attributes(&ids);
        let mut order = Vec::new();
        for (i, id) in ids.iter().enumerate() {
            if !pages.contains(&i) {
                order.push(*id);
                continue;
            }
            let size = self.page_size_of(*id);
            let (tw, th) = (size.width / cols as f32, size.height / rows as f32);
            for r in 0..rows {
                for c in 0..cols {
                    let d = [
                        c as f32 * tw,
                        r as f32 * th,
                        (c + 1) as f32 * tw,
                        (r + 1) as f32 * th,
                    ];
                    let rect = display_rect_to_pdf(&size, d);
                    let mut dict = self.inner.get_dictionary(*id)?.clone();
                    dict.set(
                        "CropBox",
                        Object::Array(rect.iter().map(|v| (*v).into()).collect()),
                    );
                    // Annotations stay with the first tile only.
                    if r + c > 0 {
                        dict.remove(b"Annots");
                    }
                    let nid = if r + c == 0 {
                        *self.inner.get_dictionary_mut(*id)? = dict;
                        *id
                    } else {
                        self.inner.add_object(dict)
                    };
                    order.push(nid);
                }
            }
        }
        self.set_page_order(order)?;
        self.commit()
    }

    /// Turn a page of another document into a form XObject of this document.
    fn import_page_as_form(&mut self, other: &Document, page: usize) -> Result<(ObjectId, [f32; 4])> {
        let mut src = lopdf::Document::load_mem(other.bytes().as_slice())?;
        src.renumber_objects_with(self.inner.max_id + 1);
        let src_page = *src
            .get_pages()
            .values()
            .nth(page)
            .ok_or(Error::InvalidPage(page))?;
        // Resolve inherited attributes.
        let mut resources = Object::Dictionary(Dictionary::new());
        let mut media = [0.0f32, 0.0, 612.0, 792.0];
        let mut cur = Some(src_page);
        let mut found_res = false;
        let mut found_box = false;
        while let Some(id) = cur {
            let d = src.get_dictionary(id)?.clone();
            if !found_res {
                if let Ok(r) = d.get(b"Resources") {
                    resources = r.clone();
                    found_res = true;
                }
            }
            if !found_box {
                if let Some(r) = d
                    .get(b"CropBox")
                    .or_else(|_| d.get(b"MediaBox"))
                    .ok()
                    .and_then(|b| rect_of(&src, b))
                {
                    media = r;
                    found_box = true;
                }
            }
            cur = d.get(b"Parent").ok().and_then(|p| p.as_reference().ok());
        }
        let content = src.get_page_content(src_page);
        let max_id = src.max_id;
        // Bring every object except the page tree itself.
        for (id, obj) in std::mem::take(&mut src.objects) {
            let is_tree = obj
                .as_dict()
                .map(|d| d.has_type(b"Page") || d.has_type(b"Pages") || d.has_type(b"Catalog"))
                .unwrap_or(false);
            if !is_tree {
                self.inner.objects.insert(id, obj);
            }
        }
        self.inner.max_id = self.inner.max_id.max(max_id);
        let form = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => media.iter().map(|v| Object::from(*v)).collect::<Vec<_>>(),
                "Resources" => resources,
            },
            content,
        );
        let id = self.inner.add_object(form);
        Ok((id, media))
    }

    /// Draw the pages of `other` behind (`behind = true`) or over the given pages,
    /// scaled to fit each page. Page `i` uses page `i mod n` of `other`.
    pub fn overlay_document(&mut self, other: &Document, pages: &[usize], behind: bool) -> Result<()> {
        self.overlay_document_on_layer(other, pages, behind, None)
    }

    /// Like [`Document::overlay_document`], with the drawn pages on a layer.
    pub(crate) fn overlay_document_on_layer(
        &mut self,
        other: &Document,
        pages: &[usize],
        behind: bool,
        layer: Option<ObjectId>,
    ) -> Result<()> {
        self.check_import_from(other)?;
        self.check(pages)?;
        let n = other.page_count();
        if n == 0 {
            return Err(Error::Invalid("the overlay document has no pages".into()));
        }
        self.begin_edit();
        let ids = self.page_ids();
        let mut forms: Vec<Option<(ObjectId, [f32; 4])>> = vec![None; n];
        for (k, &p) in pages.iter().enumerate() {
            let src = k % n;
            if forms[src].is_none() {
                let (fid, fb) = self.import_page_as_form(other, src)?;
                if let (Some(oc), Ok(Object::Stream(s))) = (layer, self.inner.get_object_mut(fid)) {
                    s.dict.set("OC", Object::Reference(oc));
                }
                forms[src] = Some((fid, fb));
            }
            let (fid, fb) = forms[src].unwrap();
            let size = self.page_size_of(ids[p]);
            let m = size.crop_box;
            let (fw, fh) = (fb[2] - fb[0], fb[3] - fb[1]);
            let s = ((m[2] - m[0]) / fw).min((m[3] - m[1]) / fh);
            let tx = m[0] + ((m[2] - m[0]) - fw * s) / 2.0 - fb[0] * s;
            let ty = m[1] + ((m[3] - m[1]) - fh * s) / 2.0 - fb[1] * s;
            let name = format!("FoliOverlay{}", fid.0);
            self.add_page_resource(ids[p], "XObject", &name, Object::Reference(fid))?;
            let f = fmt_num;
            let content =
                format!("q {} 0 0 {} {} {} cm /{} Do Q\n", f(s), f(s), f(tx), f(ty), name).into_bytes();
            if behind {
                self.prepend_page_content(ids[p], content, Some("Underlay"))?;
            } else {
                self.append_page_content(ids[p], content, Some("Overlay"))?;
            }
        }
        self.commit()
    }

    /// Replace `count` pages starting at `first` with pages `src_pages` of `other`
    /// (one undo step).
    pub fn replace_pages(
        &mut self,
        first: usize,
        count: usize,
        other: &Document,
        src_pages: &[usize],
    ) -> Result<()> {
        let n = self.page_count();
        if count == 0 || first >= n {
            return Err(Error::InvalidPage(first));
        }
        if first + count > n {
            return Err(Error::InvalidPage(first + count - 1));
        }
        if src_pages.is_empty() {
            return Err(Error::Invalid("no source pages selected".into()));
        }
        let m = other.page_count();
        if let Some(p) = src_pages.iter().find(|p| **p >= m) {
            return Err(Error::InvalidPage(*p));
        }
        self.insert_document(first, other, Some(src_pages))?;
        let start = first + src_pages.len();
        let old: Vec<usize> = (start..start + count).collect();
        self.delete_pages(&old)?;
        self.merge_last_undo();
        Ok(())
    }

    /// Put a stream before the existing page content (drawn behind it).
    pub(crate) fn prepend_page_content(
        &mut self,
        page_id: ObjectId,
        mut content: Vec<u8>,
        tag: Option<&str>,
    ) -> Result<ObjectId> {
        // Wrap in q/Q so graphics state never leaks into the page content.
        let mut wrapped = b"q\n".to_vec();
        wrapped.append(&mut content);
        wrapped.extend_from_slice(b"\nQ\n");
        let stream_id = self.append_page_content(page_id, wrapped, tag)?;
        let page = self.inner.get_dictionary_mut(page_id)?;
        if let Ok(Object::Array(arr)) = page.get_mut(b"Contents") {
            if let Some(last) = arr.pop() {
                arr.insert(0, last);
            }
        }
        Ok(stream_id)
    }

    /// Fill the page background with a colour (drawn behind the content).
    pub fn add_background_color(&mut self, pages: &[usize], color: [f32; 3], opacity: f32) -> Result<()> {
        self.check(pages)?;
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            let size = self.page_size_of(ids[p]);
            let m = size.media_box;
            let f = fmt_num;
            let mut content = String::new();
            if opacity < 0.999 {
                let name = format!("FoliBg{}", (opacity.clamp(0.0, 1.0) * 100.0).round() as i32);
                let gs = dictionary! { "Type" => "ExtGState", "ca" => opacity, "CA" => opacity };
                self.add_page_resource(ids[p], "ExtGState", &name, Object::Dictionary(gs))?;
                content.push_str(&format!("/{name} gs "));
            }
            content.push_str(&format!(
                "{} {} {} rg {} {} {} {} re f",
                f(color[0]),
                f(color[1]),
                f(color[2]),
                f(m[0]),
                f(m[1]),
                f(m[2] - m[0]),
                f(m[3] - m[1])
            ));
            self.prepend_page_content(ids[p], content.into_bytes(), Some("Background"))?;
        }
        self.commit()
    }

    /// Set (or with `style == None` remove) the presentation transition of pages.
    pub fn set_page_transition(
        &mut self,
        pages: &[usize],
        style: Option<&str>,
        seconds: f32,
        display_seconds: Option<f32>,
    ) -> Result<()> {
        self.check(pages)?;
        if let Some(s) = style {
            if !TRANSITIONS.contains(&s) {
                return Err(Error::Invalid(format!("unknown transition {s}")));
            }
        }
        self.begin_edit();
        let ids = self.page_ids();
        for &p in pages {
            let d = self.inner.get_dictionary_mut(ids[p])?;
            match style {
                Some(s) => {
                    d.set("Trans", dictionary! { "Type" => "Trans", "S" => Object::Name(s.as_bytes().to_vec()), "D" => seconds.max(0.0) });
                }
                None => {
                    d.remove(b"Trans");
                }
            }
            match display_seconds {
                Some(s) if s > 0.0 => d.set("Dur", s),
                _ => {
                    d.remove(b"Dur");
                }
            }
        }
        self.commit()
    }

    /// Transition style of a page, if any.
    pub fn page_transition(&self, page: usize) -> Option<(String, f32)> {
        let id = self.page_id(page).ok()?;
        let d = self.inner.get_dictionary(id).ok()?;
        let t = deref(&self.inner, d.get(b"Trans").ok()?).as_dict().ok()?;
        let s = t
            .get(b"S")
            .ok()
            .and_then(|s| s.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_else(|| "R".into());
        let dur = t.get(b"D").ok().and_then(num).unwrap_or(1.0);
        Some((s, dur))
    }

    /// Page labels (`/PageLabels` number tree) as shown to users; falls back to 1, 2, 3, ...
    pub fn page_labels(&self) -> Vec<String> {
        self.page_label_parts()
            .into_iter()
            .map(|(style, prefix, value)| {
                let number = match style.as_slice() {
                    b"D" => value.to_string(),
                    b"R" => roman(value).to_uppercase(),
                    b"r" => roman(value),
                    b"A" => letters(value).to_uppercase(),
                    b"a" => letters(value),
                    _ => String::new(),
                };
                format!("{prefix}{number}")
            })
            .collect()
    }

    /// The parts of the label of every page: numbering style (`D`, `R`, `r`,
    /// `A`, `a` or empty), prefix and number (1, 2, 3, ... without labels).
    pub(crate) fn page_label_parts(&self) -> Vec<(Vec<u8>, String, i64)> {
        let n = self.page_count();
        let doc = &self.inner;
        let mut ranges: Vec<(usize, Dictionary)> = Vec::new();
        if let Some(tree) = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"PageLabels").ok())
            .map(|t| deref(doc, t))
            .and_then(|t| t.as_dict().ok())
        {
            fn walk(
                doc: &lopdf::Document,
                node: &Dictionary,
                out: &mut Vec<(usize, Dictionary)>,
                depth: usize,
            ) {
                if depth > 32 {
                    return;
                }
                if let Ok(kids) = node
                    .get(b"Kids")
                    .map(|k| deref(doc, k))
                    .and_then(|k| k.as_array())
                {
                    for k in kids {
                        if let Ok(kd) = deref(doc, k).as_dict() {
                            walk(doc, kd, out, depth + 1);
                        }
                    }
                }
                if let Ok(nums) = node
                    .get(b"Nums")
                    .map(|k| deref(doc, k))
                    .and_then(|k| k.as_array())
                {
                    for pair in nums.chunks(2) {
                        if let (Some(start), Some(Ok(d))) = (
                            pair.first().and_then(|s| s.as_i64().ok()),
                            pair.get(1).map(|d| deref(doc, d).as_dict()),
                        ) {
                            out.push((start.max(0) as usize, d.clone()));
                        }
                    }
                }
            }
            walk(doc, tree, &mut ranges, 0);
        }
        ranges.sort_by_key(|(s, _)| *s);
        (0..n)
            .map(|p| match ranges.iter().rev().find(|(s, _)| *s <= p) {
                None => (b"D".to_vec(), String::new(), (p + 1) as i64),
                Some((start, d)) => {
                    let first = d.get(b"St").ok().and_then(|s| s.as_i64().ok()).unwrap_or(1);
                    let value = first + (p - start) as i64;
                    let prefix = d
                        .get(b"P")
                        .ok()
                        .and_then(|o| crate::util::text_of(doc, o))
                        .unwrap_or_default();
                    let style = d.get(b"S").ok().and_then(|s| s.as_name().ok()).unwrap_or(b"");
                    (style.to_vec(), prefix, value)
                }
            })
            .collect()
    }

    /// Define page labels: each range starts at a zero-based page with a style
    /// (`D`, `R`, `r`, `A`, `a` or empty), a prefix and a first number.
    pub fn set_page_labels(&mut self, ranges: &[(usize, &str, &str, i64)]) -> Result<()> {
        self.begin_edit();
        if let Err(e) = self.write_page_labels(ranges) {
            self.abandon_edit();
            return Err(e);
        }
        self.commit()
    }

    /// Write the page labels (see [`Document::set_page_labels`]) without an undo step.
    pub(crate) fn write_page_labels(&mut self, ranges: &[(usize, &str, &str, i64)]) -> Result<()> {
        if ranges.is_empty() {
            self.inner.catalog_mut()?.remove(b"PageLabels");
        } else {
            let mut nums = Vec::new();
            let mut sorted = ranges.to_vec();
            sorted.sort_by_key(|r| r.0);
            for (start, style, prefix, first) in sorted {
                let mut d = Dictionary::new();
                if !style.is_empty() {
                    d.set("S", Object::Name(style.as_bytes().to_vec()));
                }
                if !prefix.is_empty() {
                    d.set("P", lopdf::text_string(prefix));
                }
                if first != 1 {
                    d.set("St", first.max(1));
                }
                nums.push(Object::Integer(start as i64));
                nums.push(Object::Dictionary(d));
            }
            self.inner
                .catalog_mut()?
                .set("PageLabels", dictionary! { "Nums" => nums });
        }
        Ok(())
    }

    /// Flatten one annotation (by its position in the page's `/Annots`).
    pub fn flatten_annotation(&mut self, page: usize, index: usize) -> Result<()> {
        if index >= self.annotations(page)?.len() {
            return Err(Error::Invalid(format!("annotation {index} does not exist")));
        }
        self.begin_edit();
        self.flatten_internal(&[page], crate::flatten::FlattenScope::All, Some((page, index)))?;
        self.commit()
    }

    /// Normalise every page to the size of the first page (content scaled to fit).
    pub fn normalize_page_sizes(&mut self) -> Result<usize> {
        let sizes = self.page_sizes();
        let Some(first) = sizes.first().copied() else {
            return Ok(0);
        };
        let differing: Vec<usize> = sizes
            .iter()
            .enumerate()
            .filter(|(_, s)| (s.width - first.width).abs() > 0.5 || (s.height - first.height).abs() > 0.5)
            .map(|(i, _)| i)
            .collect();
        if differing.is_empty() {
            return Ok(0);
        }
        self.resize_pages(&differing, first.width, first.height, true)?;
        Ok(differing.len())
    }
}

fn roman(mut n: i64) -> String {
    const TABLE: [(i64, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

fn letters(n: i64) -> String {
    if n < 1 {
        return String::new();
    }
    let idx = ((n - 1) % 26) as u8;
    let reps = ((n - 1) / 26 + 1) as usize;
    std::iter::repeat_n((b'a' + idx) as char, reps).collect()
}

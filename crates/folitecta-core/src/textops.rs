//! Operations driven by the positioned text layer: markup on text ranges,
//! redaction of search results and links from web addresses.

use crate::annots::Markup;
use crate::document::{Document, PageSize};
use crate::error::{Error, Result};
use crate::marks::display_to_pdf;
use crate::text::{SearchHit, SearchOptions};

/// Convert a display-space rectangle (`[x0, y0, x1, y1]`, y down) to PDF user space.
pub fn display_rect_to_pdf(size: &PageSize, r: [f32; 4]) -> [f32; 4] {
    let (ax, ay) = display_to_pdf(size, r[0], size.height - r[3]);
    let (bx, by) = display_to_pdf(size, r[2], size.height - r[1]);
    [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]
}

impl Document {
    /// Text of a character range of a page.
    pub fn text_range(&self, page: usize, start: usize, end: usize) -> String {
        self.text_layer(page)
            .map(|l| l.slice(start, end))
            .unwrap_or_default()
    }

    /// Add a highlight / underline / strikeout over a character range of a page.
    pub fn markup_text_range(
        &mut self,
        page: usize,
        start: usize,
        end: usize,
        kind: Markup,
        color: [f32; 3],
        author: &str,
    ) -> Result<()> {
        let layer = self.text_layer(page).ok_or(Error::InvalidPage(page))?;
        let size = self.page_size(page)?;
        let quads = layer.range_quads_pdf(&size, start, end);
        if quads.is_empty() {
            return Err(Error::Invalid("the selection contains no text".into()));
        }
        let text = layer.slice(start, end);
        self.add_text_markup(page, kind, &quads, color, author, &text)?;
        Ok(())
    }

    /// Mark a character range for replacement with `replacement`.
    pub fn replace_text_range(
        &mut self,
        page: usize,
        start: usize,
        end: usize,
        replacement: &str,
        color: [f32; 3],
        author: &str,
    ) -> Result<()> {
        let layer = self.text_layer(page).ok_or(Error::InvalidPage(page))?;
        let size = self.page_size(page)?;
        let quads = layer.range_quads_pdf(&size, start, end);
        if quads.is_empty() {
            return Err(Error::Invalid("the selection contains no text".into()));
        }
        self.add_replacement(page, &quads, replacement, color, author)
    }

    /// Place an insert-text mark before (or, with `after`, after) character
    /// `index` of a page, carrying the text to insert.
    pub fn insert_text_mark(
        &mut self,
        page: usize,
        index: usize,
        after: bool,
        text: &str,
        color: [f32; 3],
        author: &str,
    ) -> Result<lopdf::ObjectId> {
        let layer = self.text_layer(page).ok_or(Error::InvalidPage(page))?;
        let size = self.page_size(page)?;
        let glyph_of = |i: usize| layer.char_glyph.get(i).copied().flatten();
        let (g, after) = match glyph_of(index) {
            Some(g) => (g, after),
            None => match index.checked_sub(1).and_then(glyph_of) {
                Some(g) => (g, true),
                None => return Err(Error::Invalid("there is no text at this position".into())),
            },
        };
        let q = layer.glyphs[g].quad;
        let (bx, by) = if after { (q[6], q[7]) } else { (q[4], q[5]) };
        let h = ((q[0] - q[4]).powi(2) + (q[1] - q[5]).powi(2)).sqrt();
        let at = display_to_pdf(&size, bx, size.height - by);
        self.add_caret(page, at, h, text, color, author)
    }

    /// Mark a character range for redaction (one mark per line).
    pub fn redact_text_range(
        &mut self,
        page: usize,
        start: usize,
        end: usize,
        author: &str,
    ) -> Result<usize> {
        let layer = self.text_layer(page).ok_or(Error::InvalidPage(page))?;
        let size = self.page_size(page)?;
        let rects = layer.range_rects(start, end);
        for r in &rects {
            let pr = display_rect_to_pdf(&size, *r);
            self.add_redaction(page, [pr[0] - 0.5, pr[1] - 0.5, pr[2] + 0.5, pr[3] + 0.5], author)?;
        }
        Ok(rects.len())
    }

    /// Apply a text markup to every occurrence of `query`. Returns the count.
    pub fn markup_search_results(
        &mut self,
        query: &str,
        opts: SearchOptions,
        kind: Markup,
        color: [f32; 3],
        author: &str,
    ) -> Result<usize> {
        let hits = self.search(query, opts);
        self.markup_hits(&hits, kind, color, author)
    }

    /// Apply a text markup to the given search hits. Returns the count.
    pub fn markup_hits(
        &mut self,
        hits: &[SearchHit],
        kind: Markup,
        color: [f32; 3],
        author: &str,
    ) -> Result<usize> {
        let mark = self.undo_mark();
        for h in hits {
            self.markup_text_range(h.page, h.start, h.end, kind, color, author)?;
        }
        // One undo step for all of them.
        self.squash_undo(&mark);
        Ok(hits.len())
    }

    /// Mark every occurrence of `query` for redaction. Returns the count.
    pub fn redact_search_results(&mut self, query: &str, opts: SearchOptions, author: &str) -> Result<usize> {
        let hits = self.search(query, opts);
        self.redact_hits(&hits, author)
    }

    /// Mark the given search hits for redaction. Returns the count.
    pub fn redact_hits(&mut self, hits: &[SearchHit], author: &str) -> Result<usize> {
        let mark = self.undo_mark();
        for h in hits {
            self.redact_text_range(h.page, h.start, h.end, author)?;
        }
        self.squash_undo(&mark);
        Ok(hits.len())
    }

    /// Create web links over every `http(s)://` / `www.` address found in the
    /// text of the given pages (addresses already covered by a link are skipped).
    pub fn create_links_from_urls(&mut self, pages: &[usize]) -> Result<usize> {
        let layers = self.text_layers();
        let mut count = 0;
        for &p in pages {
            let Some(layer) = layers.get(p) else {
                return Err(Error::InvalidPage(p));
            };
            let size = self.page_size(p)?;
            let existing: Vec<[f32; 4]> = self.links(p)?.into_iter().map(|l| l.rect).collect();
            for (start, end, url) in layer.urls() {
                for r in layer.range_rects(start, end) {
                    let pr = display_rect_to_pdf(&size, r);
                    let covered = existing.iter().any(|e| {
                        let ix = (e[2].min(pr[2]) - e[0].max(pr[0])).max(0.0);
                        let iy = (e[3].min(pr[3]) - e[1].max(pr[1])).max(0.0);
                        ix * iy > 0.5 * (pr[2] - pr[0]) * (pr[3] - pr[1])
                    });
                    if !covered {
                        self.add_uri_link(p, pr, &url)?;
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }
}

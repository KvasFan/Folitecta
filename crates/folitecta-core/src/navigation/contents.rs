//! Tables of contents (FOL-L022): the headings of a document found by the
//! size of their text and made into bookmarks, and pages listing the
//! bookmarks with their page numbers, every entry a link, inserted into the
//! document.

use std::collections::{HashMap, HashSet};

use lopdf::{Stream, dictionary};

use crate::document::Document;
use crate::error::{Error, Result};
use crate::marks::display_to_pdf;
use crate::outline::{DestView, OutlineItem};
use crate::typeset::{Align, Fonts, Sheet, Span, Style, Typeset, nest};

/// A heading found in the text of a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    /// 1 for the largest headings, then 2, 3 and 4.
    pub level: u8,
    pub title: String,
    pub page: usize,
    /// The top of its first line (display points, y down).
    pub top: f32,
}

/// A line of text with the size most of its letters have.
struct SizedLine {
    page: usize,
    text: String,
    size: f32,
    top: f32,
    bottom: f32,
    letters: usize,
}

/// Sizes in half points, so that 11.98 and 12.02 are the same size.
fn half_points(size: f32) -> u32 {
    (size * 2.0).round().max(0.0) as u32
}

/// The text of a line without its digits, to find lines repeated on many
/// pages with their numbers changing (running heads and feet).
fn without_digits(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_ascii_digit())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Document {
    /// The lines of every page with the size of most of their letters.
    fn sized_lines(&self) -> Vec<SizedLine> {
        let layers = self.text_layers();
        let mut out = Vec::new();
        for (page, layer) in layers.iter().enumerate() {
            let chars: Vec<char> = layer.text.chars().collect();
            for l in &layer.lines {
                let (a, b) = (l.start.min(chars.len()), l.end.min(chars.len()));
                let text = chars[a..b].iter().collect::<String>();
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if text.is_empty() {
                    continue;
                }
                let mut sizes: HashMap<u32, usize> = HashMap::new();
                for i in a..b {
                    let glyph = layer
                        .char_glyph
                        .get(i)
                        .copied()
                        .flatten()
                        .and_then(|g| layer.glyphs.get(g));
                    if let Some(g) = glyph.filter(|g| !g.text.trim().is_empty()) {
                        *sizes.entry(half_points(g.size)).or_default() += 1;
                    }
                }
                let Some((size, letters)) = sizes.into_iter().max_by_key(|(s, n)| (*n, *s)) else {
                    continue;
                };
                out.push(SizedLine {
                    page,
                    text,
                    size: size as f32 / 2.0,
                    top: l.bbox[1],
                    bottom: l.bbox[3],
                    letters,
                });
            }
        }
        out
    }

    /// The headings of the document: lines clearly larger than the text most
    /// letters are set in, short, and not repeated on many pages (running
    /// heads); lines of one heading are joined. The largest size is level 1,
    /// the next ones 2, 3 and 4.
    pub fn find_headings(&self) -> Vec<Heading> {
        let lines = self.sized_lines();
        let mut weight: HashMap<u32, usize> = HashMap::new();
        for l in &lines {
            *weight.entry(half_points(l.size)).or_default() += l.letters;
        }
        let Some(body) = weight
            .into_iter()
            .max_by_key(|(s, n)| (*n, std::cmp::Reverse(*s)))
            .map(|(s, _)| s as f32 / 2.0)
        else {
            return Vec::new();
        };
        // Lines on at least three pages and a quarter of them.
        let pages = self.page_count();
        let mut on_pages: HashMap<String, HashSet<usize>> = HashMap::new();
        for l in &lines {
            on_pages
                .entry(without_digits(&l.text))
                .or_default()
                .insert(l.page);
        }
        let running = |l: &SizedLine| {
            on_pages
                .get(&without_digits(&l.text))
                .is_some_and(|p| p.len() >= 3 && p.len() * 4 >= pages)
        };
        let heading = |l: &SizedLine| {
            l.size >= body * 1.15
                && l.size >= body + 1.0
                && l.text.split_whitespace().count() <= 16
                && l.text.chars().count() <= 140
                && l.text.chars().any(|c| c.is_alphabetic())
        };
        let mut found: Vec<(u32, Heading, f32)> = Vec::new();
        for l in lines.iter().filter(|l| heading(l) && !running(l)) {
            // The next line of the same heading: same page and size, right below.
            if let Some((size, h, bottom)) = found.last_mut() {
                if h.page == l.page
                    && *size == half_points(l.size)
                    && l.top >= *bottom - 1.0
                    && l.top - *bottom < l.size * 0.9
                {
                    h.title.push(' ');
                    h.title.push_str(&l.text);
                    *bottom = l.bottom;
                    continue;
                }
            }
            found.push((
                half_points(l.size),
                Heading {
                    level: 0,
                    title: l.text.clone(),
                    page: l.page,
                    top: l.top,
                },
                l.bottom,
            ));
        }
        let mut sizes: Vec<u32> = found.iter().map(|(s, _, _)| *s).collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes.dedup();
        found
            .into_iter()
            .map(|(s, mut h, _)| {
                h.level = (sizes.iter().position(|x| *x == s).unwrap_or(0) + 1).min(4) as u8;
                h
            })
            .collect()
    }

    /// Bookmarks for `headings`, nested by level, each opening its page at
    /// the heading: in place of the bookmarks there are (`replace`) or after
    /// them. One undo step; returns how many were made.
    pub fn bookmarks_from_headings(&mut self, headings: &[Heading], replace: bool) -> Result<usize> {
        if headings.is_empty() {
            return Err(Error::Invalid("no headings to make bookmarks of".into()));
        }
        let sizes = self.page_sizes();
        let mut items = Vec::new();
        for h in headings {
            let size = sizes.get(h.page).ok_or(Error::InvalidPage(h.page))?;
            let (_, top) = display_to_pdf(size, 0.0, size.height - h.top);
            let view = DestView::Xyz {
                left: None,
                top: Some(top + 4.0),
                zoom: None,
            };
            items.push((h.level, self.new_bookmark_item(&h.title, h.page, Some(view))?));
        }
        let mut tree = nest(items);
        if !replace {
            let mut all = self.outline();
            all.append(&mut tree);
            tree = all;
        }
        self.set_outline(&tree)?;
        Ok(headings.len())
    }

    /// Insert before page `at` pages titled `title` listing the bookmarks
    /// that open a page of the document, indented by level, with dotted
    /// leaders to their page numbers (as the pages are numbered once these
    /// are in); every entry is a link to its place. The pages have the size
    /// of the first page. One undo step; returns how many pages were made.
    pub fn insert_contents(&mut self, at: usize, title: &str) -> Result<usize> {
        let n = self.page_count();
        if at > n {
            return Err(Error::InvalidPage(at));
        }
        fn walk(items: &[OutlineItem], level: u8, out: &mut Vec<(u8, String, usize, Option<DestView>)>) {
            for i in items {
                if let Some(p) = i.page {
                    out.push((level, i.title.clone(), p, i.view.clone()));
                }
                walk(&i.children, level + 1, out);
            }
        }
        let mut entries = Vec::new();
        walk(&self.outline(), 1, &mut entries);
        if entries.is_empty() {
            return Err(Error::Invalid("the document has no bookmarks to list".into()));
        }
        let first = self.page_size(0)?;
        let sheet = Sheet {
            width: first.width,
            height: first.height,
            margin: (first.width.min(first.height) * 0.09).clamp(36.0, 72.0),
        };
        // The numbers depend on how many pages the contents take.
        let mut count = 1;
        let mut ts = contents_pages(&entries, title, sheet, at, count)?;
        for _ in 0..3 {
            let made = ts.cursor().0 + 1;
            if made == count {
                break;
            }
            count = made;
            ts = contents_pages(&entries, title, sheet, at, count)?;
        }
        let (fonts, pages) = ts.into_pages();
        let views: Vec<Option<DestView>> = entries.iter().map(|e| e.3.clone()).collect();
        self.edit_step(|doc| {
            let mut ids = doc.page_ids();
            let mut new_ids = Vec::with_capacity(pages.len());
            for (content, _) in &pages {
                let content_id = doc
                    .inner
                    .add_object(Stream::new(dictionary! {}, content.clone().into_bytes()));
                new_ids.push(doc.inner.add_object(dictionary! {
                    "Type" => "Page",
                    "MediaBox" => vec![0.into(), 0.into(), sheet.width.into(), sheet.height.into()],
                    "Contents" => content_id,
                    "Resources" => dictionary! {},
                }));
            }
            let tail = ids.split_off(at);
            ids.extend(new_ids.iter().copied());
            ids.extend(tail);
            doc.set_page_order(ids)?;
            fonts.finish(doc, &new_ids)?;
            for (i, (_, links)) in pages.iter().enumerate() {
                for (rect, target) in links {
                    // "#k": the k-th entry.
                    let Some(k) = target.strip_prefix('#').and_then(|k| k.parse::<usize>().ok()) else {
                        continue;
                    };
                    let page = shifted(entries[k].2, at, pages.len());
                    let view = views[k].clone().unwrap_or(DestView::Fit);
                    let dest = doc.dest_array(page, &view)?;
                    doc.push_link_raw(at + i, *rect, dictionary! { "S" => "GoTo", "D" => dest })?;
                }
            }
            Ok(pages.len())
        })
    }
}

/// Where page `page` goes once `count` pages are put before page `at`.
fn shifted(page: usize, at: usize, count: usize) -> usize {
    if page >= at { page + count } else { page }
}

/// The contents set on pages of `sheet`, the numbers of the pages as they
/// are once `count` pages are put before page `at`.
fn contents_pages(
    entries: &[(u8, String, usize, Option<DestView>)],
    title: &str,
    sheet: Sheet,
    at: usize,
    count: usize,
) -> Result<Typeset> {
    let numbers: Vec<String> = entries
        .iter()
        .map(|e| (shifted(e.2, at, count) + 1).to_string())
        .collect();
    let texts = std::iter::once(title)
        .chain([". 0123456789"])
        .chain(entries.iter().map(|e| e.1.as_str()));
    let fonts = Fonts::new(texts)?;
    let mut ts = Typeset::new(sheet, fonts);
    let size = 11.0f32;
    let leading = size * 1.55;
    let ink = [0.1, 0.1, 0.12];
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let (left, right) = (ts.left(), ts.right());
    if !title.trim().is_empty() {
        ts.paragraph(
            &[Span::styled(title, bold)],
            size * 1.8,
            size * 2.4,
            left,
            right - left,
            ink,
            Align::Left,
        )?;
        ts.space(size * 0.8);
    }
    for (k, ((level, text, _, _), number)) in entries.iter().zip(&numbers).enumerate() {
        let style = if *level == 1 { bold } else { Style::default() };
        if *level == 1 && k > 0 {
            ts.space(size * 0.35);
        }
        let x0 = left + f32::from(level.saturating_sub(1).min(5)) * size * 1.6;
        let number_w = ts.fonts.width(number, style, size);
        let avail = (right - x0 - number_w - size * 2.0).max(size * 6.0);
        let lines = ts.break_lines(&[Span::styled(text.clone(), style)], size, avail);
        let rows = lines.len().max(1);
        ts.need(leading * rows as f32);
        let (page, top) = ts.cursor();
        ts.lines(&lines, size, leading, x0, avail, ink, Align::Left)?;
        // After the title, on its last line: dots running to the number.
        let end = x0 + lines.last().map(|l| ts.line_width(l, size)).unwrap_or(0.0);
        let dot = ts.fonts.width(" .", Style::default(), size);
        let room = right - number_w - size * 0.6 - (end + size * 0.6);
        if room > dot {
            let dots = " .".repeat((room / dot).floor() as usize);
            let w = ts.fonts.width(&dots, Style::default(), size);
            ts.put(
                &dots,
                Style::default(),
                size,
                leading,
                right - number_w - size * 0.6 - w,
                [0.55, 0.56, 0.6],
            )?;
        }
        ts.put(number, style, size, leading, right - number_w, ink)?;
        let (_, bottom) = ts.cursor();
        ts.link(page, [x0, bottom, right, top], format!("#{k}"));
    }
    Ok(ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_after_the_contents_move_on() {
        assert_eq!(shifted(0, 0, 2), 2);
        assert_eq!(shifted(3, 2, 1), 4);
        assert_eq!(shifted(1, 2, 1), 1);
    }

    #[test]
    fn running_heads_match_whatever_their_numbers() {
        assert_eq!(
            without_digits("Chapter 3 — Page 12"),
            without_digits("chapter 3 — page 47")
        );
        assert_ne!(without_digits("Introduction"), without_digits("Conclusion"));
    }
}
